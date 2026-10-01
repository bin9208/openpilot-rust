use super::*;
#[derive(Clone)]
pub struct WidgetHandle(std::rc::Rc<std::cell::RefCell<Box<dyn Widget>>>);
pub struct WeakWidgetHandle(std::rc::Weak<std::cell::RefCell<Box<dyn Widget>>>);
impl WeakWidgetHandle {
    pub fn upgrade(&self) -> Option<WidgetHandle> {
        self.0.upgrade().map(WidgetHandle)
    }
}
impl WidgetHandle {
    pub fn downgrade(&self) -> WeakWidgetHandle {
        WeakWidgetHandle(std::rc::Rc::downgrade(&self.0))
    }
    pub fn new(widget: impl Widget) -> Self {
        Self::from_box(Box::new(widget))
    }
    pub fn from_box(widget: Box<dyn Widget>) -> Self {
        Self(std::rc::Rc::new(std::cell::RefCell::new(widget)))
    }
    pub fn same(&self, other: &Self) -> bool {
        std::rc::Rc::ptr_eq(&self.0, &other.0)
    }
    pub fn borrow(&self) -> Result<std::cell::Ref<'_, Box<dyn Widget>>, Error> {
        self.0
            .try_borrow()
            .map_err(|_| Error::Contract("widget is already mutably borrowed"))
    }
    pub fn borrow_mut(&self) -> Result<std::cell::RefMut<'_, Box<dyn Widget>>, Error> {
        self.0
            .try_borrow_mut()
            .map_err(|_| Error::Contract("widget is already borrowed"))
    }
    pub fn get<T: Widget>(&self) -> Result<std::cell::Ref<'_, T>, Error> {
        std::cell::Ref::filter_map(self.borrow()?, |widget| {
            (widget.as_ref() as &dyn std::any::Any).downcast_ref::<T>()
        })
        .map_err(|_| Error::Contract("widget type mismatch"))
    }
    pub fn get_mut<T: Widget>(&self) -> Result<std::cell::RefMut<'_, T>, Error> {
        std::cell::RefMut::filter_map(self.borrow_mut()?, |widget| {
            (widget.as_mut() as &mut dyn std::any::Any).downcast_mut::<T>()
        })
        .map_err(|_| Error::Contract("widget type mismatch"))
    }
}
pub enum NavigationRequest {
    Close,
    Push(WidgetHandle),
    Pop(Option<Box<dyn FnOnce()>>),
    PopAt(usize),
    PopTo {
        target: WidgetHandle,
        instant: bool,
        callback: Option<Box<dyn FnOnce()>>,
    },
}
#[derive(Clone, Default)]
pub struct NavigationQueue(
    std::rc::Rc<std::cell::RefCell<std::collections::VecDeque<NavigationRequest>>>,
);
impl NavigationQueue {
    pub fn push(&self, request: NavigationRequest) {
        self.0.borrow_mut().push_back(request);
    }
    pub fn pop(&self) -> Option<NavigationRequest> {
        self.0.borrow_mut().pop_front()
    }
}
