use std::sync::Arc;

pub trait Viewable<R> {
    fn draw_frame(self: Arc<Self>, renderer: &mut R);
}
