//! Module for the ID map wrapper.
//!
//! Most index implementations will bind a sequential ID to each vector by default.
//! However, some specific implementations support binding each vector an arbitrary
//! ID. When supported, this can be done with the [`Index#add_with_ids`] method.
//! Please see the [Faiss wiki] for more information.
//!
//! For implementations which do not support arbitrary IDs, this module provides
//! the [`IdMap`] wrapper type. An `IdMap<I>` retains the algorithm and compile
//! time properties of the index type `I`, while ensuring the extra ID mapping
//! functionality.
//!
//! [Faiss wiki]: https://github.com/facebookresearch/faiss/wiki/Pre--and-post-processing#faiss-id-mapping
//! [`Index#add_with_id`]: ../trait.Index.html#add_with_ids
//! [`IdMap`]: struct.IdMap.html
//!
//! # Examples
//!
//! A flat index does not support arbitrary ID mapping, but `IdMap` solves this:
//!
//! ```
//! use faiss::{IdMap, Index, FlatIndex};
//! # fn run() -> Result<(), Box<::std::error::Error>>  {
//! let mut index = FlatIndex::new_l2(4)?;
//! assert!(index.add_with_ids(&[0., 1., 0., 1.], &[5]).is_err());
//!
//! let mut index = IdMap::new(index)?;
//! index.add_with_ids(&[0., 1., 0., 1.], &[5])?;
//! assert_eq!(index.ntotal(), 1);
//! # Ok(())
//! # }
//! # run().unwrap();
//! ```
//!
//! `IdMap` also works for GPU backed indexes, but the index map will reside
//! in CPU memory. Once an index map is made, moving an index to/from the GPU
//! is not possible.
//! 
//! # #[cfg(feature = "gpu")]
//! # use faiss::{GpuResources, StandardGpuResources, Index, FlatIndex, IdMap};
//! # #[cfg(feature = "gpu")]
//! # use faiss::error::Result;
//!
//! # #[cfg(feature = "gpu")]
//! # fn run() -> Result<()> {
//! let index = FlatIndex::new_l2(8)?;
//! let gpu_res = StandardGpuResources::new()?;
//! let index: IdMap<_> = IdMap::new(index.into_gpu(&gpu_res, 0)?)?;
//! # Ok(())
//! # }
//! # #[cfg(feature = "gpu")]
//! # run().unwrap()
//! ```
//! 

use error::Result;
use faiss_sys::*;
use index::{
    AssignSearchResult, ConcurrentIndex, CpuIndex, FromInnerPtr, Idx, Index, NativeIndex, RangeSearchResult,
    SearchResult,
};

use std::marker::PhantomData;
use std::mem;
use std::ptr;

/// Wrapper for implementing arbitrary ID mapping to an index.
///
/// See the [module level documentation] for more information.
///
/// [module level documentation](./index.html)
#[derive(Debug)]
pub struct IdMap<I> {
    inner: *mut FaissIndexIDMap,
    index_inner: *mut FaissIndex,
    phantom: PhantomData<I>,
}

unsafe impl<I: Send> Send for IdMap<I> {}
unsafe impl<I: Sync> Sync for IdMap<I> {}
impl<I: CpuIndex> CpuIndex for IdMap<I> {}

impl<I> NativeIndex for IdMap<I> {
    fn inner_ptr(&self) -> *mut FaissIndex {
        self.inner
    }
}

impl<I> Drop for IdMap<I> {
    fn drop(&mut self) {
        unsafe {
            faiss_Index_free(self.inner);
        }
    }
}

impl<I> IdMap<I>
where
    I: NativeIndex,
{
    /// Augment an index with arbitrary ID mapping.
    pub fn new(index: I) -> Result<Self> {
        unsafe {
            let index_inner = index.inner_ptr();
            let mut inner_ptr = ptr::null_mut();
            faiss_try!(faiss_IndexIDMap_new(&mut inner_ptr,index_inner));
            // let IDMap take ownership of the index
            faiss_IndexIDMap_set_own_fields(inner_ptr, 1);
            mem::forget(index);

            Ok(IdMap {
                inner: inner_ptr,
                index_inner,
                phantom: PhantomData,
            })
        }
    }

    /// Retrieve a slice of the internal ID map.
    pub fn id_map(&self) -> &[Idx] {
        unsafe {
            let mut id_ptr = ptr::null_mut();
            let mut psize = 0;
            faiss_IndexIDMap_id_map(self.inner, &mut id_ptr, &mut psize);
            ::std::slice::from_raw_parts(id_ptr, psize)
        }
    }

    /// Obtain the raw pointer to the internal index.
    /// 
    /// # Safety
    /// 
    /// While this method is safe, note that the returned index pointer is
    /// already owned by this ID map. Therefore, it is undefined behaviour to
    /// create a high-level index value from this pointer without first
    /// decoupling this ownership. See [`into_inner`] for a safe alternative.
    pub fn index_inner_ptr(&self) -> *mut FaissIndex {
        self.index_inner
    }

    /// Discard the ID map, recovering the index originally created without it.
    pub fn into_inner(self) -> I
    where
        I: FromInnerPtr,
    {
        unsafe {
            // make id map disown the index
            faiss_IndexIDMap_set_own_fields(self.inner, 0);
            // now it's safe to build a managed index
            I::from_inner_ptr(self.index_inner)
        }
    }
}

impl<I> Index for IdMap<I> {
    fn is_trained(&self) -> bool {
        unsafe { faiss_Index_is_trained(self.inner_ptr()) != 0 }
    }

    fn ntotal(&self) -> u64 {
        unsafe { faiss_Index_ntotal(self.inner_ptr()) as u64 }
    }

    fn d(&self) -> u32 {
        unsafe { faiss_Index_d(self.inner_ptr()) as u32 }
    }

    fn metric_type(&self) -> ::metric::MetricType {
        unsafe {
            ::metric::MetricType::from_code(faiss_Index_metric_type(self.inner_ptr()) as u32)
                .unwrap()
        }
    }

    fn add(&mut self, x: &[f32]) -> Result<()> {
        unsafe {
            let n = x.len() / self.d() as usize;
            faiss_try!(faiss_Index_add(self.inner_ptr(), n as i64, x.as_ptr()));
            Ok(())
        }
    }

    fn add_with_ids(&mut self, x: &[f32], xids: &[::index::Idx]) -> Result<()> {
        unsafe {
            let n = x.len() / self.d() as usize;
            faiss_try!(faiss_Index_add_with_ids(
                self.inner_ptr(),
                n as i64,
                x.as_ptr(),
                xids.as_ptr()
            ));
            Ok(())
        }
    }
    fn train(&mut self, x: &[f32]) -> Result<()> {
        unsafe {
            let n = x.len() / self.d() as usize;
            faiss_try!(faiss_Index_train(self.inner_ptr(), n as i64, x.as_ptr()));
            Ok(())
        }
    }
    fn assign(&mut self, query: &[f32], k: usize) -> Result<::index::AssignSearchResult> {
        unsafe {
            let nq = query.len() / self.d() as usize;
            let mut out_labels = vec![0 as ::index::Idx; k * nq];
            faiss_try!(faiss_Index_assign(
                self.inner_ptr(),
                nq as idx_t,
                query.as_ptr(),
                out_labels.as_mut_ptr(),
                k as i64
            ));
            Ok(::index::AssignSearchResult { labels: out_labels })
        }
    }
    fn search(&mut self, query: &[f32], k: usize) -> Result<::index::SearchResult> {
        unsafe {
            let nq = query.len() / self.d() as usize;
            let mut distances = vec![0_f32; k * nq];
            let mut labels = vec![0 as ::index::Idx; k * nq];
            faiss_try!(faiss_Index_search(
                self.inner_ptr(),
                nq as idx_t,
                query.as_ptr(),
                k as idx_t,
                distances.as_mut_ptr(),
                labels.as_mut_ptr()
            ));
            Ok(::index::SearchResult { distances, labels })
        }
    }
    fn range_search(&mut self, query: &[f32], radius: f32) -> Result<::index::RangeSearchResult> {
        unsafe {
            let nq = (query.len() / self.d() as usize) as idx_t;
            let mut p_res: *mut FaissRangeSearchResult = ::std::ptr::null_mut();
            faiss_try!(faiss_RangeSearchResult_new(&mut p_res, nq));
            faiss_try!(faiss_Index_range_search(
                self.inner_ptr(),
                nq,
                query.as_ptr(),
                radius,
                p_res
            ));
            Ok(::index::RangeSearchResult { inner: p_res })
        }
    }

    fn reset(&mut self) -> Result<()> {
        unsafe {
            faiss_try!(faiss_Index_reset(self.inner_ptr()));
            Ok(())
        }
    }
}

impl<I> ConcurrentIndex for IdMap<I>
where
    I: ConcurrentIndex,
{
    fn assign(&self, query: &[f32], k: usize) -> Result<AssignSearchResult> {
        unsafe {
            let nq = query.len() / self.d() as usize;
            let mut out_labels = vec![0 as Idx; k * nq];
            faiss_try!(faiss_Index_assign(
                self.inner,
                nq as idx_t,
                query.as_ptr(),
                out_labels.as_mut_ptr(),
                k as i64
            ));
            Ok(AssignSearchResult { labels: out_labels })
        }
    }
    fn search(&self, query: &[f32], k: usize) -> Result<SearchResult> {
        unsafe {
            let nq = query.len() / self.d() as usize;
            let mut distances = vec![0_f32; k * nq];
            let mut labels = vec![0 as Idx; k * nq];
            faiss_try!(faiss_Index_search(
                self.inner,
                nq as idx_t,
                query.as_ptr(),
                k as idx_t,
                distances.as_mut_ptr(),
                labels.as_mut_ptr()
            ));
            Ok(SearchResult { distances, labels })
        }
    }
    fn range_search(&self, query: &[f32], radius: f32) -> Result<RangeSearchResult> {
        unsafe {
            let nq = (query.len() / self.d() as usize) as idx_t;
            let mut p_res: *mut FaissRangeSearchResult = ptr::null_mut();
            faiss_try!(faiss_RangeSearchResult_new(&mut p_res, nq));
            faiss_try!(faiss_Index_range_search(
                self.inner,
                nq,
                query.as_ptr(),
                radius,
                p_res
            ));
            Ok(RangeSearchResult { inner: p_res })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::IdMap;
    use index::{index_factory, Index};
    use MetricType;

    #[test]
    fn flat_index_search_ids() {
        let index = index_factory(8, "Flat", MetricType::L2).unwrap();
        let some_data = &[
            7.5_f32, -7.5, 7.5, -7.5, 7.5, 7.5, 7.5, 7.5, -1., 1., 1., 1., 1., 1., 1., -1., 0., 0.,
            0., 1., 1., 0., 0., -1., 100., 100., 100., 100., -100., 100., 100., 100., 120., 100.,
            100., 105., -100., 100., 100., 105.,
        ];
        let some_ids = &[3, 6, 9, 12, 15];
        let mut index = IdMap::new(index).unwrap();
        index.add_with_ids(some_data, some_ids).unwrap();
        assert_eq!(index.ntotal(), 5);

        let my_query = [0.; 8];
        let result = index.search(&my_query, 5).unwrap();
        assert_eq!(result.labels, vec![9, 6, 3, 12, 15]);
        assert!(result.distances.iter().all(|x| *x > 0.));

        let my_query = [100.; 8];
        let result = index.search(&my_query, 5).unwrap();
        assert_eq!(result.labels, vec![12, 15, 3, 6, 9]);
        assert!(result.distances.iter().all(|x| *x > 0.));

        let my_query = vec![
            0., 0., 0., 0., 0., 0., 0., 0., 100., 100., 100., 100., 100., 100., 100., 100.,
        ];
        let result = index.search(&my_query, 5).unwrap();
        assert_eq!(result.labels, vec![9, 6, 3, 12, 15, 12, 15, 3, 6, 9]);
        assert!(result.distances.iter().all(|x| *x > 0.));
    }
}