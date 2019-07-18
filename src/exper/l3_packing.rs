#![feature(specialization)]
#![feature(asm)] 
#![allow(unused_imports)]

extern crate core;
extern crate momms;

use std::time::{Instant};
use typenum::{Unsigned,U1};

use momms::kern::KernelNM;
use momms::matrix::{Scalar, Mat, ColumnPanelMatrix, RowPanelMatrix, Matrix, Hierarch};
use momms::composables::{GemmNode, AlgorithmStep, PartM, PartN, PartK, PackA, PackB, UnpackC, SpawnThreads, ParallelM, ParallelN, TheRest};
use momms::thread_comm::ThreadInfo;
use momms::util;

fn test_algorithm_flat<T: Scalar, S: GemmNode<T, Matrix<T>, Matrix<T>, Matrix<T>>>
    ( m:usize, n: usize, k: usize, algo: &mut S, flusher: &mut Vec<f64>, n_reps: usize ) -> (f64, T) 
{
    let mut best_time: f64 = 9999999999.0;
    let mut worst_err: T = T::zero();

    for _ in 0..n_reps {
        //Create matrices.
        let mut a = <Matrix<T>>::new(m, k);
        let mut b = <Matrix<T>>::new(k, n);

        //The micro-kernel used is more efficient with row-major C.
        let mut c = <Matrix<T>>::new(n, m);
        c.transpose();

        //Fill the matrices
        a.fill_rand(); c.fill_zero(); b.fill_rand();

        //Read a buffer so that A, B, and C are cold in cache.
        for i in flusher.iter_mut() { *i += 1.0; }

        //Time and run algorithm
        let start = Instant::now();
        unsafe{ algo.run( &mut a, &mut b, &mut c, &ThreadInfo::single_thread() ); }
        best_time = best_time.min(util::dur_seconds(start));
        let err = util::test_c_eq_a_b( &mut a, &mut b, &mut c);
        worst_err = worst_err.max(err);
    }
    (best_time, worst_err)
}

fn test() {
    use typenum::{UInt, UTerm, B0, B1, Unsigned};
    type U3000 = UInt<UInt<typenum::U750, B0>, B0>;
    type NC = U3000;
    type KC = typenum::U192; 
    type MC = typenum::U120; 
    type MR = typenum::U4;
    type NR = typenum::U12;

    type Goto<T,MTA,MTB,MTC> 
        = SpawnThreads<T, MTA, MTB, MTC,
          PartN<T, MTA, MTB, MTC, NC,
          PartK<T, MTA, MTB, MTC, KC,
          PackB<T, MTA, MTB, MTC, ColumnPanelMatrix<T,NR>,
          PartM<T, MTA, ColumnPanelMatrix<T,NR>, MTC, MC,
          PackA<T, MTA, ColumnPanelMatrix<T,NR>, MTC, RowPanelMatrix<T,MR>,
          ParallelN<T, RowPanelMatrix<T,MR>, ColumnPanelMatrix<T,NR>, MTC, NR, TheRest,
          KernelNM<T, RowPanelMatrix<T,MR>, ColumnPanelMatrix<T,NR>, MTC, NR, MR>>>>>>>>;

    type NcL3 = typenum::U768;
    type KcL3 = typenum::U768;
    type McL2 = typenum::U120;
    type KcL2 = typenum::U192;
    type L3bA<T> = Hierarch<T, MR, KcL2, U1, MR>;
    type L3bB<T> = Hierarch<T, KcL2, NR, NR, U1>;
    type L3bC<T> = Hierarch<T, MR, NR, NR, U1>;

    type L3B<T,MTA,MTB,MTC> 
        = SpawnThreads<T, MTA, MTB, MTC,
          PartN<T, MTA, MTB, MTC, NcL3,
          PartK<T, MTA, MTB, MTC, KcL3,
          PackB<T, MTA, MTB, MTC, L3bB<T>,
          PartM<T, MTA, L3bB<T>, MTC, McL2,
          UnpackC<T, MTA, L3bB<T>, MTC, L3bC<T>, //not sure if this goes here or the beginning or never...
          PartK<T, MTA, L3bB<T>, L3bC<T>, KcL2,
          PackA<T, MTA, L3bB<T>, L3bC<T>, L3bA<T>,
          ParallelN<T, L3bA<T>, L3bB<T>, L3bC<T>, NR, TheRest,
          KernelNM<T, L3bA<T>, L3bB<T>, L3bC<T>, NR, MR>>>>>>>>>>;

    let mut goto = <Goto<f64, Matrix<f64>, Matrix<f64>, Matrix<f64>>>::new();
    let mut l3b = <L3B<f64, Matrix<f64>, Matrix<f64>, Matrix<f64>>>::new();
    goto.set_n_threads(4);
    l3b.set_n_threads(4);

    //Initialize array to flush cache with
    let flusher_len = 2*1024*1024; //16MB
    let mut flusher : Vec<f64> = Vec::with_capacity(flusher_len);
    for _ in 0..flusher_len { flusher.push(0.0); }

    println!("m\tn\tk\t{: <13}{: <13}{: <15}{: <15}", "goto", "l3b", "goto", "l3b");
    for index in 01..81 {
        let size = index*50;
        let (m, n, k) = (size, size, size);

        let n_reps = 5;
        let (goto_time, goto_err) = test_algorithm_flat(m, n, k, &mut goto, &mut flusher, n_reps);
        let (l3b_time, l3b_err) = test_algorithm_flat(m, n, k, &mut l3b, &mut flusher, n_reps);

        println!("{}\t{}\t{}\t{}{}{}{}", 
                 m, n, k,
                 format!("{: <13.5}", util::gflops(m,n,k,goto_time)), 
                 format!("{: <13.5}", util::gflops(m,n,k,l3b_time)), 
                 format!("{: <15.5e}", goto_err.sqrt()),
                 format!("{: <15.5e}", l3b_err.sqrt()));

    }

    let mut sum = 0.0;
    for a in flusher.iter() {
        sum += *a;
    }
    println!("Flush value {}", sum);
}

fn main() {
    test( );
}
