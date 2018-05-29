extern crate alloc;

use thread_comm::ThreadInfo;
use typenum::Unsigned;
use self::alloc::heap::{Alloc,Global};
use matrix::{Scalar,Mat,ResizableBuffer,RoCM};
use super::view::{MatrixView};
use util::capacity_to_aligned_layout;
use core::{self, ptr,marker::PhantomData};

use composables::{AlgorithmStep};

pub struct RowPanelMatrix<T: Scalar, PH: Unsigned> {
    alpha: T,

    y_views: Vec<MatrixView>, //offset is in # of panels
    x_views: Vec<MatrixView>,

    panel_stride: usize,
    buffer: *mut T,
    capacity: usize,
    is_alias: bool,

    _pht: PhantomData<PH>,
}
impl<T: Scalar, PH: Unsigned> RowPanelMatrix<T,PH> {
    pub fn new(h: usize, w: usize) -> RowPanelMatrix<T,PH> {
        assert_ne!(core::mem::size_of::<T>(), 0, "Matrix can't handle ZSTs");
    
        //Figure out the number of panels
        let panel_h = PH::to_usize();
        let mut n_panels = h / panel_h;
        if n_panels * panel_h < h {
            n_panels += 1;
        }
        let capacity = (n_panels+1) * panel_h * w; //Extra panel for ``preloading'' in ukernel

        let mut y_views : Vec<MatrixView> = Vec::with_capacity(16);
        let mut x_views : Vec<MatrixView> = Vec::with_capacity(16);
        y_views.push(MatrixView{ offset: 0, padding: 0, iter_size: h }); 
        x_views.push(MatrixView{ offset: 0, padding: 0, iter_size: w }); 

        let layout = capacity_to_aligned_layout::<T>(capacity);
        let buf =  unsafe {
            Global.alloc(layout).expect("Could not allocate buffer for matrix!")
        };

        RowPanelMatrix{ alpha: T::one(),
                        y_views: y_views, x_views: x_views,
                        panel_stride: panel_h*w, 
                        buffer: buf.as_ptr() as *mut _,
                        capacity: capacity,
                        is_alias: false,
                        _pht: PhantomData }
    }
    

    #[inline(always)]
    pub fn get_panel_stride(&self) -> usize { self.panel_stride }
    
    #[inline(always)]
    pub unsafe fn get_panel(&mut self, id: usize) -> *mut T {
        let y_view = self.y_views.last().unwrap();
        self.buffer.offset(((y_view.offset + id)*self.panel_stride) as isize)
    }
}
impl<T: Scalar, PH: Unsigned> Mat<T> for RowPanelMatrix<T, PH> {
    #[inline(always)]
    fn get(&self, y: usize, x: usize) -> T {
        let panel_h = PH::to_usize();
        let y_view = self.y_views.last().unwrap();
        let x_view = self.x_views.last().unwrap();

        let panel_id = y / panel_h;
        let panel_index  = y % panel_h;
        let elem_index = (panel_id + y_view.offset) * self.panel_stride + (x + x_view.offset) * panel_h + panel_index;
        unsafe{
            ptr::read(self.buffer.offset(elem_index as isize))
        }
    }
    #[inline(always)]
    fn set(&mut self, y: usize, x: usize, alpha: T) {
        let panel_h = PH::to_usize();
        let y_view = self.y_views.last().unwrap();
        let x_view = self.x_views.last().unwrap();

        let panel_id = y / panel_h;
        let panel_index  = y % panel_h;
        let elem_index = (panel_id + y_view.offset) * self.panel_stride + (x + x_view.offset) * panel_h + panel_index;
        unsafe{
            ptr::write(self.buffer.offset(elem_index as isize), alpha);
        }
    }
    #[inline(always)]
    fn iter_height(&self) -> usize {
        let y_view = self.y_views.last().unwrap();
        y_view.iter_size
    }
    #[inline(always)]
    fn iter_width(&self) -> usize {
        let x_view = self.x_views.last().unwrap();
        x_view.iter_size
    }

    #[inline(always)]
    fn logical_h_padding(&self) -> usize {
        let y_view = self.y_views.last().unwrap();
        y_view.padding
    }
    #[inline(always)]
    fn logical_w_padding(&self) -> usize {
        let x_view = self.x_views.last().unwrap();
        x_view.padding
    }

    #[inline(always)]
    fn set_scalar(&mut self, alpha: T) {
        self.alpha = alpha;
    }
    #[inline(always)]
    fn get_scalar(&self) -> T {
        self.alpha
    }

    fn push_y_split(&mut self, start: usize, end: usize) {
        debug_assert!(start % PH::to_usize() == 0 && end % PH::to_usize() == 0);

        let zoomed_view = {
            let uz_view = self.y_views.last().unwrap();
            let new_padding = if end <= self.height() { 0 } else { end - self.height() };
            let new_offset = uz_view.offset + start / PH::to_usize();
            MatrixView{ offset: new_offset, padding: new_padding, iter_size: end-start }
        };
        self.y_views.push(zoomed_view);
    }

    fn push_x_split(&mut self, start: usize, end: usize) {
        let zoomed_view = {
            let uz_view = self.x_views.last().unwrap();
            let new_padding = if end <= self.width() { 0 } else { end - self.width() };
            let new_offset = uz_view.offset + start;
            MatrixView{ offset: new_offset, padding: new_padding, iter_size: end-start }
        };
        self.x_views.push(zoomed_view);
    }
    
    #[inline(always)]
    fn pop_y_split(&mut self) {
        debug_assert!(self.y_views.len() >= 2);
        self.y_views.pop();
    }

    #[inline(always)]
    fn pop_x_split(&mut self) {
        debug_assert!(self.x_views.len() >= 2);
        self.x_views.pop();
    }
    
    fn push_y_view(&mut self, blksz: usize) -> usize{
        let (zoomed_view, uz_iter_size) = {
            let uz_view = self.y_views.last().unwrap();
            let (z_iter_size, z_padding) = uz_view.zoomed_size_and_padding(0, blksz);
            (MatrixView{ offset: uz_view.offset, padding: z_padding, iter_size: z_iter_size }, uz_view.iter_size)
        };
        self.y_views.push(zoomed_view);
        uz_iter_size
    }

    fn push_x_view(&mut self, blksz: usize) -> usize {
        let (zoomed_view, uz_iter_size) = {
            let uz_view = self.x_views.last().unwrap();
            let (z_iter_size, z_padding) = uz_view.zoomed_size_and_padding(0, blksz);
            (MatrixView{ offset: uz_view.offset, padding: z_padding, iter_size: z_iter_size }, uz_view.iter_size)
        };
        self.x_views.push(zoomed_view);
        uz_iter_size
    }

    #[inline(always)]
    fn pop_y_view(&mut self) {
        debug_assert!(self.y_views.len() >= 2);
        self.y_views.pop();
    }
    #[inline(always)]
    fn pop_x_view(&mut self) {
        debug_assert!(self.x_views.len() >= 2);
        self.x_views.pop();
    }
    
    fn slide_y_view_to(&mut self, y: usize, blksz: usize) {
        let view_len = self.y_views.len();
        debug_assert!(view_len >= 2);

        let uz_view = self.y_views[view_len-2];
        let(z_iter_size, z_padding) = uz_view.zoomed_size_and_padding(y, blksz);

        let z_view = self.y_views.last_mut().unwrap();
        z_view.iter_size = z_iter_size;
        z_view.padding = z_padding;
        z_view.offset = uz_view.offset + y / PH::to_usize();
    }

    fn slide_x_view_to(&mut self, x: usize, blksz: usize) {
        let view_len = self.x_views.len();
        debug_assert!(view_len >= 2);

        let uz_view = self.x_views[view_len-2];
        let(z_iter_size, z_padding) = uz_view.zoomed_size_and_padding(x, blksz);

        let z_view = self.x_views.last_mut().unwrap();
        z_view.iter_size = z_iter_size;
        z_view.padding = z_padding;
        z_view.offset = uz_view.offset + x;
    }
    
    #[inline(always)]
    unsafe fn make_alias(&self) -> Self {
        let y_view = self.y_views.last().unwrap();
        let x_view = self.x_views.last().unwrap();

        let mut y_views_alias : Vec<MatrixView> = Vec::with_capacity(16);
        let mut x_views_alias : Vec<MatrixView> = Vec::with_capacity(16);
        y_views_alias.push(MatrixView{ offset: y_view.offset, padding: y_view.offset, iter_size: y_view.iter_size });
        x_views_alias.push(MatrixView{ offset: x_view.offset, padding: x_view.offset, iter_size: x_view.iter_size });

        RowPanelMatrix{ alpha: self.alpha,
                        y_views: y_views_alias, x_views: x_views_alias,
                        panel_stride: self.panel_stride,
                        buffer: self.buffer, 
                        capacity: self.capacity,
                        is_alias: true,
                        _pht: PhantomData }
    }

    #[inline(always)]
    unsafe fn send_alias(&mut self, thr: &ThreadInfo<T>) {
        let buf = thr.broadcast(self.buffer);
        self.is_alias = true;
        self.buffer = buf;
    }
}
impl<T:Scalar, PH: Unsigned> Drop for RowPanelMatrix<T, PH> {
    fn drop(&mut self) {
        if !self.is_alias {
            unsafe {
                let layout = capacity_to_aligned_layout::<T>(self.capacity);
                Global.dealloc(ptr::NonNull::new(self.buffer as *mut _).unwrap(), layout);
            }
        }
    }
}

unsafe impl<T:Scalar, PH: Unsigned> Send for RowPanelMatrix<T, PH> {}

impl<T:Scalar, PH: Unsigned> ResizableBuffer<T> for RowPanelMatrix<T, PH> {
    #[inline(always)]
    fn empty(_: AlgorithmStep, _: AlgorithmStep, _: &[AlgorithmStep]) -> Self {
        RowPanelMatrix::new(0,0)
    }
    #[inline(always)]
    fn capacity(&self) -> usize { self.capacity }
    #[inline(always)]
    fn set_capacity(&mut self, capacity: usize) { self.capacity = capacity; }
    #[inline(always)]
    fn capacity_for(other: &Mat<T>, _: AlgorithmStep, _: AlgorithmStep, _: &[AlgorithmStep]) -> usize {
        if other.height() == 0 || other.width() == 0 { 
            0   
        } else {
            let new_n_panels = (other.height()-1) / PH::to_usize() + 1;
            (new_n_panels + 1) * PH::to_usize() * other.width()
        }
    }
    #[inline(always)]
    fn aquire_buffer_for(&mut self, req_capacity: usize) {
        let req_padded_capacity = req_capacity;
        if req_padded_capacity > self.capacity {
            unsafe {
                let new_layout = capacity_to_aligned_layout::<T>(req_padded_capacity);
                self.buffer = Global.realloc(ptr::NonNull::new(self.buffer as *mut _).expect("Attempting to deallocate null buffer for matrix"),
                    new_layout, req_padded_capacity).expect("Could not allocate buffer for matrix!").as_ptr() as *mut _;
                self.capacity = req_padded_capacity;
            }
        }
    }
    #[inline(always)]
    fn resize_to(&mut self, other: &Mat<T>, _: AlgorithmStep, _: AlgorithmStep, _: &[AlgorithmStep]) {
        debug_assert_eq!(self.y_views.len(), 1, "Can't resize a submatrix!");
        let y_view = self.y_views.last_mut().unwrap();
        let x_view = self.x_views.last_mut().unwrap();

        y_view.iter_size = other.iter_height();
        x_view.iter_size = other.iter_width();
        y_view.padding = other.logical_h_padding();
        x_view.padding = other.logical_w_padding();
        self.panel_stride = PH::to_usize()*other.width();
    }
}

impl<T: Scalar, PH: Unsigned> RoCM<T> for RowPanelMatrix<T,PH> {
    #[inline(always)]
    fn partition_is_rocm(&self) -> bool { 
        self.height() <= PH::to_usize()
    }

    #[inline(always)]
    fn get_leaf_rs(&self) -> usize { 
        1
    }

    #[inline(always)]
    fn get_leaf_cs(&self) -> usize { 
        PH::to_usize()
    }

    #[inline(always)]
    unsafe fn get_buffer(&self) -> *const T {
        let panel_h = PH::to_usize();
        let y_view = self.y_views.last().unwrap();
        let x_view = self.x_views.last().unwrap();

        self.buffer.offset((x_view.offset*panel_h + y_view.offset*self.panel_stride) as isize)
    }

    #[inline(always)]
    unsafe fn get_mut_buffer(&mut self) -> *mut T {
        let panel_h = PH::to_usize();
        let y_view = self.y_views.last().unwrap();
        let x_view = self.x_views.last().unwrap();

        self.buffer.offset((x_view.offset*panel_h + y_view.offset*self.panel_stride) as isize)
    }

	#[inline(always)]
    fn get_block_rs(&self, lvl: usize, blksz: usize) -> usize {
        if lvl == 0 {
            1
        } else {
			debug_assert_eq!(blksz % PH::to_usize(), 0);
			self.panel_stride * blksz / PH::to_usize()
        }
    }

	#[inline(always)]
    fn get_block_cs(&self, _: usize, blksz: usize) -> usize {
		blksz * PH::to_usize()
    }
    #[inline(always)]
    fn full_leaves() -> bool {
        false
    }

    #[inline(always)]
    unsafe fn establish_leaf(&mut self, _y: usize, _x: usize, _height: usize, _width: usize) { }
}
