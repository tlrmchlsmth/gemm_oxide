extern crate alloc;

use thread_comm::ThreadInfo;
use self::alloc::heap::{Alloc,Global};
use matrix::{Scalar,Mat,RoCM};
use super::view::{MatrixView};
use core::{self,ptr};

pub struct Matrix<T: Scalar> {
    //Matrix scalar
    alpha: T,

    //Stack of views of the matrix
    y_views: Vec<MatrixView>,
    x_views: Vec<MatrixView>,

    //Strides and buffer
    row_stride: usize,
    column_stride: usize,
    buffer: *mut T,
    capacity: usize,
    is_alias: bool,
}
impl<T: Scalar> Matrix<T> {
    pub fn new(h: usize, w: usize) -> Matrix<T> {
        assert_ne!(core::mem::size_of::<T>(), 0, "Matrix can't handle ZSTs");
        let layout = ::util::capacity_to_aligned_layout::<T>(h * w);
        let buf =
            unsafe {
                Global.alloc(layout).expect("Could not allocate buffer for matrix!")
            };

        let mut y_views : Vec<MatrixView> = Vec::with_capacity(16);
        let mut x_views : Vec<MatrixView> = Vec::with_capacity(16);
        y_views.push(MatrixView{ offset: 0, padding: 0, iter_size: h });
        x_views.push(MatrixView{ offset: 0, padding: 0, iter_size: w });

        Matrix{ alpha: T::one(),
                y_views: y_views,
                x_views: x_views,
                row_stride: 1, column_stride: h,
                buffer: buf.as_ptr() as *mut _,
                capacity: h * w,
                is_alias: false }
    }

    #[inline(always)] pub fn get_row_stride(&self) -> usize { self.row_stride }
    #[inline(always)] pub fn get_column_stride(&self) -> usize { self.column_stride }

    pub fn transpose(&mut self) {
        if self.y_views.len() != 1 || self.x_views.len() != 1 { panic!("can't transpose a submatrix!") };
        let xview = self.x_views.pop().unwrap();
        let yview = self.y_views.pop().unwrap();
        self.y_views.push(xview);
        self.x_views.push(yview);
        
        core::mem::swap(&mut self.column_stride, &mut self.row_stride);
    }
}
impl<T: Scalar> Mat<T> for Matrix<T> {
    #[inline(always)]
    fn get(&self, y: usize, x: usize) -> T {
        let y_view = self.y_views.last().unwrap();
        let x_view = self.x_views.last().unwrap();

        let y_coord = (y + y_view.offset) * self.row_stride;
        let x_coord = (x + x_view.offset) * self.column_stride;
        unsafe{
            ptr::read(self.buffer.offset((y_coord + x_coord) as isize))
        }
    }
    #[inline(always)]
    fn set(&mut self, y: usize, x: usize, alpha: T) {
        let y_view = self.y_views.last().unwrap();
        let x_view = self.x_views.last().unwrap();

        let y_coord = (y + y_view.offset) * self.row_stride;
        let x_coord = (x + x_view.offset) * self.column_stride;
        unsafe{
            ptr::write(self.buffer.offset((y_coord + x_coord) as isize), alpha);
        }
    }
    #[inline(always)]
    fn iter_height(&self) -> usize {
        self.y_views.last().unwrap().iter_size 
    }
    #[inline(always)]
    fn iter_width(&self) -> usize { 
        self.x_views.last().unwrap().iter_size 
    }
    #[inline(always)]
    fn logical_h_padding(&self) -> usize { 
        self.y_views.last().unwrap().padding 
    }
    #[inline(always)]
    fn logical_w_padding(&self) -> usize { 
        self.x_views.last().unwrap().padding 
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
        let zoomed_view = {
            let uz_view = self.y_views.last().unwrap();
            //Determine new padding.
            let new_padding = if end <= self.height() { 0 } else { end - self.height() };
            //Determine out new offset
            let new_offset = uz_view.offset + start;
            MatrixView{ offset: new_offset, padding: new_padding, iter_size: end-start }
        };
        self.y_views.push(zoomed_view);
    }
    fn push_x_split(&mut self, start: usize, end: usize) {
        let zoomed_view = {
            let uz_view = self.x_views.last().unwrap();
            //Determine new padding.
            let new_padding = if end <= self.width() { 0 } else { end - self.width() };
            //Determine out new offset
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

    fn push_y_view(&mut self, blksz: usize) -> usize {
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
        z_view.offset = uz_view.offset + y;
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
       let x_view = self.x_views.last().unwrap();
       let y_view = self.y_views.last().unwrap();

       let mut x_views_alias : Vec<MatrixView> = Vec::with_capacity(16);
       let mut y_views_alias : Vec<MatrixView> = Vec::with_capacity(16);
       x_views_alias.push(MatrixView{ offset: x_view.offset, padding: x_view.offset, iter_size: x_view.iter_size });
       y_views_alias.push(MatrixView{ offset: y_view.offset, padding: y_view.offset, iter_size: y_view.iter_size });

        Matrix{ alpha: self.alpha,
                x_views: x_views_alias, y_views: y_views_alias,
                row_stride: self.row_stride, column_stride: self.column_stride,
                buffer: self.buffer,
                capacity: self.capacity,
                is_alias: true }
    }

    #[inline(always)]
    unsafe fn send_alias(&mut self, thr: &ThreadInfo<T>) {
        let buf = thr.broadcast(self.buffer);
        self.is_alias = true;
        self.buffer = buf;
    }
}
impl<T:Scalar> Drop for Matrix<T> {
    fn drop(&mut self) {
        unsafe {
            if !self.is_alias {
                let layout = ::util::capacity_to_aligned_layout::<T>(self.capacity);
                Global.dealloc(ptr::NonNull::new(self.buffer as *mut _).unwrap(), layout);
            }
        }
    }
}
unsafe impl<T:Scalar> Send for Matrix<T> {}


impl<T: Scalar> RoCM<T> for Matrix<T> {
    #[inline(always)]
    fn partition_is_rocm(&self) -> bool { true }

    #[inline(always)]
    fn get_leaf_rs(&self) -> usize { self.row_stride }

    #[inline(always)]
    fn get_leaf_cs(&self) -> usize { self.column_stride }
    
    #[inline(always)]
    unsafe fn get_buffer(&self) -> *const T {
        let y_view = self.y_views.last().unwrap();
        let x_view = self.x_views.last().unwrap();

        self.buffer.offset((y_view.offset*self.row_stride + x_view.offset*self.column_stride) as isize)
    }   

    #[inline(always)]
    unsafe fn get_mut_buffer(&mut self) -> *mut T {
        let y_view = self.y_views.last().unwrap();
        let x_view = self.x_views.last().unwrap();

        self.buffer.offset((y_view.offset*self.row_stride + x_view.offset*self.column_stride) as isize)
    }
    #[inline(always)]
    fn get_block_rs(&self, _: usize, blksz: usize) -> usize {
        blksz * self.row_stride
    }
    #[inline(always)]
    fn get_block_cs(&self, _: usize, blksz: usize) -> usize {
        blksz * self.column_stride
    }
    #[inline(always)]
    fn full_leaves() -> bool {
        false
    }

    #[inline(always)]
    unsafe fn establish_leaf(&mut self, _x: usize, _y: usize, _height: usize, _width: usize) { }
}
