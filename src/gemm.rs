use matrix::{Scalar,Mat,ResizeableBuffer,ColumnPanelMatrix,RowPanelMatrix};
use core::marker::{PhantomData};
use pack::{Copier,Packer};
use thread::{ThreadInfo};

extern crate core;

pub trait GemmNode<T: Scalar, At: Mat<T>, Bt: Mat<T>, Ct: Mat<T>> {
    #[inline(always)]
    unsafe fn run( &mut self, a: &mut At, b: &mut Bt, c: &mut Ct, thr: &ThreadInfo<T> ) -> ();
    #[inline(always)]
    unsafe fn shadow( &self ) -> Self where Self: Sized;
}

pub struct PackA<T: Scalar, At: Mat<T>, Bt: Mat<T>, Ct: Mat<T>, APt: Mat<T>, 
    S: GemmNode<T, APt, Bt, Ct>> {
    child: S,
    packer: Packer<T, At, APt>,
    a_pack: APt,
    _t: PhantomData<T>,
    _at: PhantomData<At>,
    _bt: PhantomData<Bt>,
    _ct: PhantomData<Ct>,
    _apt: PhantomData<APt>,
} 
impl<T: Scalar,At: Mat<T>, Bt: Mat<T>, Ct: Mat<T>, APt: Mat<T>, S: GemmNode<T, APt, Bt, Ct>> PackA <T,At,Bt,Ct,APt,S> 
    where APt: ResizeableBuffer<T> {
    pub fn new( child: S ) -> PackA<T, At, Bt, Ct, APt, S>{
        PackA{ child: child, 
               a_pack: APt::empty(), packer: Packer::new(),
               _t: PhantomData, _at:PhantomData, _bt: PhantomData, _ct: PhantomData, _apt: PhantomData }
    }
}
impl<T: Scalar, At: Mat<T>, Bt: Mat<T>, Ct: Mat<T>, APt: Mat<T>, S: GemmNode<T, APt, Bt, Ct>>
    GemmNode<T, At, Bt, Ct> for PackA<T, At, Bt, Ct, APt, S>
    where APt: ResizeableBuffer<T> {
    #[inline(always)]
    unsafe fn run( &mut self, a: &mut At, b: &mut Bt, c:&mut Ct, thr: &ThreadInfo<T> ) -> () {
        thr.barrier();
        if self.a_pack.capacity() < APt::capacity_for(a) {
            if thr.thread_id() == 0 {
                self.a_pack.aquire_buffer_for(APt::capacity_for(a));
            }
            else {
                self.a_pack.set_capacity( APt::capacity_for(a) );
            }
            self.a_pack.send_alias( thr );
        }
        self.a_pack.resize_to( a );
        self.packer.pack( a, &mut self.a_pack, thr );
        thr.barrier();
        self.child.run(&mut self.a_pack, b, c, thr);
    }
    #[inline(always)]
    unsafe fn shadow( &self ) -> Self where Self: Sized {
        PackA{ child: self.child.shadow(), 
               a_pack: APt::empty(), 
               packer: Packer::new(),
               _t: PhantomData, _at:PhantomData, _bt: PhantomData, _ct: PhantomData,
               _apt: PhantomData }
    }
}

pub struct PackB<T: Scalar, At: Mat<T>, Bt: Mat<T>, Ct: Mat<T>, BPt: Mat<T>, 
    S: GemmNode<T, At, BPt, Ct>> {
    child: S,
    packer: Packer<T, Bt, BPt>,
    b_pack: BPt,
    _t: PhantomData<T>,
    _at: PhantomData<At>,
    _bt: PhantomData<Bt>,
    _ct: PhantomData<Ct>,
    _bpt: PhantomData<BPt>,
} 
impl<T: Scalar,At: Mat<T>, Bt: Mat<T>, Ct: Mat<T>, BPt: Mat<T>, S: GemmNode<T, At, BPt, Ct>> PackB <T,At,Bt,Ct,BPt,S> 
    where BPt: ResizeableBuffer<T> {
    pub fn new( child: S ) -> PackB<T, At, Bt, Ct, BPt, S>{
        PackB{ child: child, 
               b_pack: BPt::empty(), packer: Packer::new(),
               _t: PhantomData, _at:PhantomData, _bt: PhantomData, _ct: PhantomData, _bpt: PhantomData }
    }
}
impl<T: Scalar, At: Mat<T>, Bt: Mat<T>, Ct: Mat<T>, BPt: Mat<T>, S: GemmNode<T, At, BPt, Ct>>
    GemmNode<T, At, Bt, Ct> for PackB<T, At, Bt, Ct, BPt, S>
    where BPt: ResizeableBuffer<T> {
    #[inline(always)]
    unsafe fn run( &mut self, a: &mut At, b: &mut Bt, c:&mut Ct, thr: &ThreadInfo<T> ) -> () {
        thr.barrier();
        if self.b_pack.capacity() < BPt::capacity_for(b) {
            if thr.thread_id() == 0 {
                self.b_pack.aquire_buffer_for(BPt::capacity_for(b));
            }
            else {
                self.b_pack.set_capacity( BPt::capacity_for(b) );
            }
            self.b_pack.send_alias( thr );
        }
        self.b_pack.resize_to( b );
        self.packer.pack( b, &mut self.b_pack, thr );
        thr.barrier();
        self.child.run(a, &mut self.b_pack, c, thr);
    }
    #[inline(always)]
    unsafe fn shadow( &self ) -> Self where Self: Sized {
        PackB{ child: self.child.shadow(), 
               b_pack: BPt::empty(), 
               packer: Packer::new(),
               _t: PhantomData, _at:PhantomData, _bt: PhantomData, _ct: PhantomData,
               _bpt: PhantomData }
    }
}
pub struct PartM<T: Scalar, At: Mat<T>, Bt: Mat<T>, Ct: Mat<T>, S: GemmNode<T, At, Bt, Ct>> {
    bsz: usize,
    child: S,
    _t: PhantomData<T>,
    _at: PhantomData<At>,
    _bt: PhantomData<Bt>,
    _ct: PhantomData<Ct>,
}
impl<T: Scalar, At: Mat<T>, Bt: Mat<T>, Ct: Mat<T>, S: GemmNode<T, At, Bt, Ct>> PartM<T,At,Bt,Ct,S> {
    pub fn new( bsz: usize, child: S ) -> PartM<T, At, Bt, Ct,S>{
            PartM{ bsz: bsz, child: child, 
                   _t: PhantomData, _at: PhantomData, _bt: PhantomData, _ct: PhantomData }
    }
}
impl<T: Scalar, At: Mat<T>, Bt: Mat<T>, Ct: Mat<T>, S: GemmNode<T, At, Bt, Ct>>
    GemmNode<T, At, Bt, Ct> for PartM<T,At,Bt,Ct,S> {
    #[inline(always)]
    unsafe fn run( &mut self, a: &mut At, b: &mut Bt, c: &mut Ct, thr: &ThreadInfo<T> ) -> () {
        let m_save = c.iter_height();
        let ay_off_save = a.off_y();
        let cy_off_save = c.off_y();
        
        let mut i = 0;
        while i < m_save  {
            a.adjust_y_view( m_save, ay_off_save, self.bsz, i);
            c.adjust_y_view( m_save, cy_off_save, self.bsz, i);

            self.child.run(a, b, c, thr);
            i += self.bsz;
        }

        a.set_iter_height( m_save );
        a.set_off_y( ay_off_save );
        c.set_iter_height( m_save );
        c.set_off_y( cy_off_save );
    }
    #[inline(always)]
    unsafe fn shadow( &self ) -> Self where Self: Sized {
        PartM{ bsz: self.bsz, child: self.child.shadow(), 
               _t: PhantomData, _at: PhantomData, _bt: PhantomData, _ct: PhantomData }
    }
}

pub struct PartN<T: Scalar, At: Mat<T>, Bt: Mat<T>, Ct: Mat<T>, S: GemmNode<T, At, Bt, Ct>> {
    bsz: usize,
    child: S,
    _t: PhantomData<T>,
    _at: PhantomData<At>,
    _bt: PhantomData<Bt>,
    _ct: PhantomData<Ct>,
}
impl<T: Scalar, At: Mat<T>, Bt: Mat<T>, Ct: Mat<T>, S: GemmNode<T, At, Bt, Ct>> PartN<T,At,Bt,Ct,S> {
    pub fn new( bsz: usize, child: S ) -> PartN<T, At, Bt, Ct, S>{
            PartN{ bsz: bsz, child: child, 
                   _t: PhantomData, _at: PhantomData, _bt: PhantomData, _ct: PhantomData }
    }
}
impl<T: Scalar, At: Mat<T>, Bt: Mat<T>, Ct: Mat<T>, S: GemmNode<T, At, Bt, Ct>>
    GemmNode<T, At, Bt, Ct> for PartN<T,At,Bt,Ct,S> {
    #[inline(always)]
    unsafe fn run( &mut self, a: &mut At, b: &mut Bt, c: &mut Ct, thr:&ThreadInfo<T> ) -> () {
        let n_save = c.iter_width();
        let bx_off_save = b.off_x();
        let cx_off_save = c.off_x();
        
        let mut i = 0;
        while i < n_save {
            b.adjust_x_view( n_save, bx_off_save, self.bsz, i);
            c.adjust_x_view( n_save, cx_off_save, self.bsz, i);

            self.child.run(a, b, c, thr);
            i += self.bsz;
        }

        b.set_iter_width( n_save );
        b.set_off_x( bx_off_save );
        c.set_iter_width( n_save );
        c.set_off_x( cx_off_save );
    }
    #[inline(always)]
    unsafe fn shadow( &self ) -> Self where Self: Sized {
        PartN{ bsz: self.bsz, child: self.child.shadow(), 
               _t: PhantomData, _at: PhantomData, _bt: PhantomData, _ct: PhantomData }
    }
}

pub struct PartK<T: Scalar, At: Mat<T>, Bt: Mat<T>, Ct: Mat<T>, S: GemmNode<T, At, Bt, Ct>> {
    bsz: usize,
    child: S,
    _t: PhantomData<T>,
    _at: PhantomData<At>,
    _bt: PhantomData<Bt>,
    _ct: PhantomData<Ct>,
}
impl<T: Scalar, At: Mat<T>, Bt: Mat<T>, Ct: Mat<T>, S: GemmNode<T, At, Bt, Ct>> PartK<T,At,Bt,Ct,S> {
    pub fn new( bsz: usize, child: S ) -> PartK<T, At, Bt, Ct, S>{
        PartK{ bsz: bsz, child: child, 
               _t: PhantomData, _at: PhantomData, _bt: PhantomData, _ct: PhantomData }
    }
}
impl<T: Scalar, At: Mat<T>, Bt: Mat<T>, Ct: Mat<T>, S: GemmNode<T, At, Bt, Ct>>
    GemmNode<T, At, Bt, Ct> for PartK<T,At,Bt,Ct,S> {
    #[inline(always)]
    unsafe fn run( &mut self, a: &mut At, b: &mut Bt, c: &mut Ct, thr: &ThreadInfo<T> ) -> () {
        let k_save = a.iter_width();
        let ax_off_save = a.off_x();
        let by_off_save = b.off_y();
        
        let mut i = 0;
        while i < k_save  {
            a.adjust_x_view( k_save, ax_off_save, self.bsz, i);
            b.adjust_y_view( k_save, by_off_save, self.bsz, i);

            self.child.run(a, b, c, thr);
            i += self.bsz;
        }

        a.set_iter_width( k_save );
        a.set_off_x( ax_off_save );
        b.set_iter_height( k_save );
        b.set_off_y( by_off_save );
    }
    #[inline(always)]
    unsafe fn shadow( &self ) -> Self where Self: Sized {
        PartK{ bsz: self.bsz, child: self.child.shadow(), 
               _t: PhantomData, _at: PhantomData, _bt: PhantomData, _ct: PhantomData }
    }
}

pub struct TripleLoop{}
impl TripleLoop {
    pub fn new() -> TripleLoop {
        TripleLoop{}
    }
}
impl<T: Scalar, At: Mat<T>, Bt: Mat<T>, Ct: Mat<T>> 
    GemmNode<T, At, Bt, Ct> for TripleLoop {
    #[inline(always)]
    unsafe fn run( &mut self, a: &mut At, b: &mut Bt, c: &mut Ct, _thr: &ThreadInfo<T> ) -> () {
        //For now, let's do an axpy based gemm
        for x in 0..c.width() {
            for z in 0..a.width() {
                for y in 0..c.height() {
                    let t = a.get(y,z) * b.get(z,x) + c.get(y,x);
                    c.set( y, x, t );
                }
            }
        }
    }
    #[inline(always)]
    unsafe fn shadow( &self ) -> Self where Self: Sized {
        TripleLoop{}
    }
}
