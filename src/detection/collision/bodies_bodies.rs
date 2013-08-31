use std::ptr;
use std::num::{Zero, One};
use std::borrow;
use nalgebra::mat::{Translation, Rotate, Rotation, Transform, Inv};
use nalgebra::vec::{Vec, AlgebraicVecExt, Cross, Dim};
use ncollide::geom::AnnotatedPoint;
use ncollide::broad;
use ncollide::broad::{InterferencesBroadPhase, RayCastBroadPhase};
use ncollide::narrow::algorithm::johnson_simplex::{RecursionTemplate, JohnsonSimplex};
use ncollide::narrow::{CollisionDetector, GeomGeom};
use ncollide::ray::{Ray, RayCastWithTransform};
use object::{Body, ToRigidBody, RB, SB};
use detection::constraint::{Constraint, RBRB};
use detection::detector::Detector;
use signal::signal::SignalEmiter;

pub enum PairwiseDetector<N, LV, AV, M, II> {
    GG(GeomGeom<N, LV, AV, M, II>),
    Unsuported
}

struct Dispatcher<N, LV, AV, M, II> {
    simplex: JohnsonSimplex<N, AnnotatedPoint<LV>>
}

impl<N:  Clone + Zero,
     LV: Clone + Zero + Dim,
     AV,
     M,
     II>
Dispatcher<N, LV, AV, M, II> {
    pub fn new() -> Dispatcher<N, LV, AV, M, II> {
        let template = RecursionTemplate::new(Dim::dim(None::<LV>));
        let simplex  = JohnsonSimplex::new(template);
        Dispatcher {
            simplex: simplex
        }
    }
}
impl<N: NumCast + Zero + Clone, LV: Clone, AV, M, II>
     broad::Dispatcher<Body<N, LV, AV, M, II>, PairwiseDetector<N, LV, AV, M, II>>
for Dispatcher<N, LV, AV, M, II> {
    fn dispatch(&self, a: &Body<N, LV, AV, M, II>, b: &Body<N, LV, AV, M, II>)
        -> PairwiseDetector<N, LV, AV, M, II> {
        match (*a, *b) {
            (RB(rb1), RB(rb2)) => {
                GG(GeomGeom::new(rb1.geom(), rb2.geom(), &self.simplex))
            },
            _ => Unsuported
        }
    }

    fn is_valid(&self,
                a: &Body<N, LV, AV, M, II>,
                b: &Body<N, LV, AV, M, II>)
                -> bool {
        if borrow::ref_eq(a, b) {
            return false
        }

        match (*a, *b) {
            (RB(a), RB(b)) => a.can_move() || b.can_move(),
            _ => true
        }
    }
}


pub struct BodiesBodies<N, LV, AV, M, II, BF> {
    broad_phase: @mut BF,
    update_bf:   bool
}

impl<N:  'static + ApproxEq<N> + Num + Real + Float + Ord + Clone + Algebraic + ToStr,
     LV: 'static + AlgebraicVecExt<N> + Cross<AV> + ApproxEq<N> + Translation<LV> + Clone + ToStr +
         Rotate<LV> + Transform<LV>,
     AV: 'static + Vec<N> + ToStr,
     M:  'static + Translation<LV> + Mul<M, M> + Rotate<LV> + Rotation<AV> + Inv + Transform<LV> + One,
     II: 'static,
     BF: 'static + InterferencesBroadPhase<Body<N, LV, AV, M, II>, PairwiseDetector<N, LV, AV, M, II>>>
BodiesBodies<N, LV, AV, M, II, BF> {
    pub fn new(events:    @mut SignalEmiter<N, Body<N, LV, AV, M, II>, Constraint<N, LV, AV, M, II>>,
               bf:        @mut BF,
               update_bf: bool) -> @mut BodiesBodies<N, LV, AV, M, II, BF> {
        let res = @mut BodiesBodies {
            broad_phase: bf,
            update_bf:   update_bf
        };

        events.add_body_activated_handler(ptr::to_mut_unsafe_ptr(res) as uint, |b, out| res.activate(b, out));
        events.add_body_deactivated_handler(ptr::to_mut_unsafe_ptr(res) as uint, |b| res.deactivate(b));

        res
    }

    fn activate(&mut self,
                body: @mut Body<N, LV, AV, M, II>,
                out:  &mut ~[Constraint<N, LV, AV, M, II>]) {
        let mut collector = ~[];

        do self.broad_phase.activate(body) |b1, b2, cd| {
            match *cd {
                GG(ref mut d) => {
                    let rb1 = b1.to_rigid_body_or_fail();
                    let rb2 = b2.to_rigid_body_or_fail();

                    // FIXME: is the update needed? Or do we have enough guarantees to avoid it?
                    d.update(rb1.transform_ref(), rb1.geom(), rb2.transform_ref(), rb2.geom());

                    d.colls(&mut collector);

                    for c in collector.iter() {
                        out.push(RBRB(b1, b2, c.clone()))
                    }

                    collector.clear()
                },
                Unsuported => { }
            }
            
        }
    }

    fn deactivate(&mut self, body: @mut Body<N, LV, AV, M, II>) {
        self.broad_phase.deactivate(body)
    }


}

impl<N:  'static + Clone + Zero + Orderable + NumCast + Algebraic + Primitive + Float + ToStr,
     LV: 'static + AlgebraicVecExt<N> + Clone + ToStr,
     AV: 'static,
     M:  'static + Translation<LV> + Mul<M, M> + Rotate<LV> + Transform<LV>,
     II: 'static,
     BF: RayCastBroadPhase<LV, Body<N, LV, AV, M, II>>>
BodiesBodies<N, LV, AV, M, II, BF> {
    pub fn interferences_with_ray(&mut self,
                                  ray: &Ray<LV>,
                                  out: &mut ~[(@mut Body<N, LV, AV, M, II>, N)]) {
        let mut bodies = ~[];

        self.broad_phase.interferences_with_ray(ray, &mut bodies);

        for b in bodies.iter() {
            match **b {
                RB(rb) => {
                    match rb.geom().toi_with_transform_and_ray(rb.transform_ref(), ray) {
                        None    => { },
                        Some(t) => out.push((*b, t))
                    }
                },
                SB(_) => fail!("Not yet implemented.")
            }
        }
    }
}

impl<N:  'static + ApproxEq<N> + Num + Real + Float + Ord + Clone + Algebraic + ToStr,
     LV: 'static + AlgebraicVecExt<N> + Cross<AV> + ApproxEq<N> + Translation<LV> + Clone + ToStr +
         Rotate<LV> + Transform<LV>,
     AV: 'static + Vec<N> + ToStr,
     M:  'static + Rotation<AV> + Rotate<LV> + Translation<LV> + Transform<LV> + One + Mul<M, M> + Inv,
     II: 'static,
     BF: InterferencesBroadPhase<Body<N, LV, AV, M, II>, PairwiseDetector<N, LV, AV, M, II>>>
Detector<N, Body<N, LV, AV, M, II>, Constraint<N, LV, AV, M, II>>
for BodiesBodies<N, LV, AV, M, II, BF> {
    fn add(&mut self, o: @mut Body<N, LV, AV, M, II>) {
        if self.update_bf {
            self.broad_phase.add(o);
        }
    }

    fn remove(&mut self, o: @mut Body<N, LV, AV, M, II>) {
        if self.update_bf {
            self.broad_phase.remove(o);
        }
    }

    fn update(&mut self) {
        if self.update_bf {
            self.broad_phase.update();
        }

        do self.broad_phase.for_each_pair_mut |b1, b2, cd| {
            match *cd {
                GG(ref mut d) => {
                    let rb1 = b1.to_rigid_body_or_fail();
                    let rb2 = b2.to_rigid_body_or_fail();

                    d.update(rb1.transform_ref(), rb1.geom(), rb2.transform_ref(), rb2.geom())
                },
                Unsuported => { }
            }
        }
    }

    fn interferences(&mut self, out: &mut ~[Constraint<N, LV, AV, M, II>]) {
        let mut collector = ~[];

        do self.broad_phase.for_each_pair_mut |b1, b2, cd| {
            match *cd {
                GG(ref mut d) => {
                    d.colls(&mut collector);

                    for c in collector.iter() {
                        out.push(RBRB(b1, b2, c.clone()))
                    }

                    collector.clear()
                },
                Unsuported => { }
            }
        }
    }

    #[inline]
    fn priority(&self) -> f64 { 50.0 }
}
