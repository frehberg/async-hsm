//! The asynchronous hierarchical state machine (HSM)
//!
//! This state machine  is an programmatic approach. It uses features of async-await
//! functionality, and combines it with usage of the process stack and scoping.
//!
//! Instead of storing state as variable, the state is represented by an async function.
//! The transition from one state to new state is a sequence of async function invocations, either
//! within a single 'Composite' or lifting back to the parent 'Composite', and so on.
//!
//! The following state machine diagram is implemented by the following code, entering the menu and
//! couting the amount of ping-pongs.
//!```text
//! @startuml res/hierarchy
//! [*] --> App
//! state App {
//! [*] --> Menu
//! state Menu {
//! }
//!
//! state Play {
//!   [*] --> Ping
//!
//!   Ping --> Ping : ping
//!   Ping --> Pong : ping
//!   Pong --> Pong : pong
//!   Pong --> Ping : ping
//! }
//! Menu --> Play: play
//! Play --> Menu: menu
//! }
//! App --> [*]: terminate
//! @enduml
//!```
//!
//!<div class="column"></div>
//!<img src="https://raw.githubusercontent.com/frehberg/async-hsm/main/res/hierarchy.svg"
//!alt="[ping pong state diagram https://github.com/frehberg/async-hsm/blob/main/res/hierarchy.svg]" />
//!</div>
//!
//!See here the [ping pong state diagram](https://raw.githubusercontent.com/frehberg/async-hsm/main/res/hierarchy.svg)
//!
//!```rust
//!     use async_std::prelude::*;
//!     use async_std::stream;
//!     use async_std::task;
//!     use async_hsm::{Composite, Transit, Builder, BuilderPair};
//!     use std::rc::Rc;
//!     use std::cell::RefCell;
//!
//!     type Score = u32;
//!     type EnterStateScore = u32;
//!     type AppComposite = Composite<AppData>;
//!     type PlayComposite = Composite<AppData>;
//!
//!
//!     type AppTransit<'s> = Transit<'s, AppComposite, Score, AppError>;
//!     type PlayTransit<'s> = Transit<'s, PlayComposite, BuilderPair<AppComposite, EnterStateScore, Score, AppError>, AppError>;
//!
//!     type AppBuilder = Builder<AppComposite, EnterStateScore, Score, AppError>;
//!     type AppBuilderPair = BuilderPair<AppComposite, EnterStateScore, Score, AppError>;
//!
//!     #[derive(Debug, Clone, PartialEq)]
//!     enum AppError { Failure }
//!
//!     static TO_MENU: AppBuilder = || |comp, score| Box::pin(menu(comp, score));
//!     static TO_PLAY: AppBuilder = || |comp, score| Box::pin(play(comp, score));
//!     static TERMINATE: AppBuilder = || |comp, score| Box::pin(terminate(comp, score));
//!
//!     #[derive(Debug, Clone, PartialEq)]
//!     enum IoEvent { Ping, Pong, Terminate, Menu, Play }
//!
//!     #[derive(Debug, Clone)]
//!     struct AppData {
//!         event: Rc<RefCell<stream::FromIter<std::vec::IntoIter<IoEvent>>>>,
//!     }
//!
//!     async fn pong<'s>(comp: &'s mut PlayComposite, score: Score) -> Result<PlayTransit<'s>, AppError> {
//!         let mut score = score + 1;
//!         let event = comp.data.event.clone();
//!         while let Some(event) = (*event).borrow_mut().next().await {
//!             match event {
//!                 IoEvent::Ping => return Ok(Transit::To(Box::pin(ping(comp, score)))),
//!                 IoEvent::Terminate => return Ok(Transit::Lift((TERMINATE, score))),
//!                 _ => score += 1,
//!             }
//!         }
//!         Ok(Transit::Lift((TERMINATE, score)))
//!     }
//!
//!     async fn ping<'s>(comp: &'s mut PlayComposite, score: Score) -> Result<PlayTransit<'s>, AppError> {
//!         let mut score = score + 1;
//!         let event = comp.data.event.clone();
//!         while let Some(event) = (*event).borrow_mut().next().await {
//!             match event {
//!                 IoEvent::Pong => return Ok(Transit::To(Box::pin(pong(comp, score)))),
//!                 IoEvent::Terminate => return Ok(Transit::Lift((TERMINATE, score))),
//!                 _ => score += 1,
//!             }
//!         }
//!         Ok(Transit::Lift((TERMINATE, score)))
//!     }
//!
//!     async fn terminate<'s>(_comp: &'s mut AppComposite, score: Score) -> Result<AppTransit<'s>, AppError> {
//!         Ok(Transit::Lift(score))
//!     }
//!
//!     async fn play<'s>(comp: &'s mut AppComposite, score: Score) -> Result<AppTransit<'s>, AppError> {
//!         let event = comp.data.event.clone();
//!         let mut play = PlayComposite::new(AppData { event: event });
//!         let (builder, build_arg): AppBuilderPair = play.init(ping, score).await?;
//!         builder()(comp, build_arg).await
//!     }
//!
//!     async fn menu<'s>(comp: &'s mut AppComposite, score: Score) -> Result<AppTransit<'s>, AppError> {
//!         let score = score;
//!         let event = comp.data.event.clone();
//!         while let Some(event) = (*event).borrow_mut().next().await {
//!             match event {
//!                 IoEvent::Play => return Ok(Transit::To(Box::pin(play(comp, score)))),
//!                 IoEvent::Terminate => return Ok(Transit::Lift(score)),
//!                 _ => continue,
//!             }
//!         }
//!         Ok(Transit::Lift(score))
//!     }
//!
//!     #[test]
//!     fn test_game() {
//!         let sequence = vec![IoEvent::Play, IoEvent::Ping, IoEvent::Pong,
//!                             IoEvent::Ping, IoEvent::Pong, IoEvent::Terminate];
//!         let event = Rc::new(RefCell::new(stream::from_iter(sequence)));
//!         let start_score = 0;
//!         let mut app = AppComposite::new(AppData { event: event });
//!         let result: Result<Score, AppError> = task::block_on(app.init(menu, start_score));
//!         assert_eq!(Ok(5), result);
//!     }
//! ```
use std::pin::Pin;
use std::future::{Future};

/// Abstract builder, a function constructing an async state function
///
/// This function are returned in case a composite ist terminated, In the outer scope/composite
/// thus function will generate the successing state.
///
/// Instances of this function must be decalred as static functions.
pub type Builder<Composite, BuildArg, Out, Err> = fn()
    -> for<'c> fn(&'c mut Composite, data: BuildArg)
        -> Pin<Box<dyn Future<Output=Result<Transit<'c, Composite,  Out, Err>, Err>> + 'c>>;

/// Pair of a factory funtion of type Builder<..>  and tha e factory argument
///
/// First element is the builder-function, the second element is used as input for the builder function.
pub type BuilderPair<Composite, BuildArg, Out, Err> = (Builder<Composite, BuildArg, Out, Err>, BuildArg);

/// Abstract handle of the successing state
///
/// This type must be returned by all async functions forming the HSM.
pub type Handle<'s, Composite, Out, Err> = Pin<Box<dyn Future<Output=Result<Transit<'s, Composite, Out, Err>, Err>> + 's>>;

/// Strucuture refering to next state within Composite or Lifter that will create instance in parent Composite
pub enum Transit<'s, Composite,  Out, Err>
    where Out: Sized + Copy
{
    /// Refering to the next state within the Composite
    To(Handle<'s, Composite, Out,  Err>),
    /// From current composite lift to outer composite and enter the state formed by "Out"
    Lift(Out),
}

/// The structure may be used to share data between states within the same Composite
pub struct Composite<Data> {
    pub data: Data,
}

/// Implementing Composite methods
impl<Data> Composite<Data> {
    /// Create a new Composite instance, sharing the data between all states within the Composite
    pub fn new(data: Data) -> Self {
        Composite { data: data }
    }

    /// Composition of states, only one sub-state at a time. The function f is initializing the  first sub state.
    ///
    /// #Examples
    /// ```
    ///     use async_std::prelude::*;
    ///     use async_std::stream;
    ///     use async_std::task;
    ///     use async_hsm::{Composite, Transit, Builder, BuilderPair};
    ///     use std::rc::Rc;
    ///     use std::cell::RefCell;
    ///
    ///     type Score = u32;
    ///     type EnterStateScore = u32;
    ///     type AppComposite = Composite<AppData>;
    ///     type AppTransit<'s> = Transit<'s, AppComposite, Score, AppError>;
    ///     type AppBuilder = Builder<AppComposite, EnterStateScore, Score, AppError>;
    ///     type AppBuilderPair = BuilderPair<AppComposite, EnterStateScore, Score, AppError>;
    ///
    ///     #[derive(Debug, Clone, PartialEq)]
    ///     enum AppError { Failure }
    ///
    ///     #[derive(Debug, Clone, PartialEq)]
    ///     enum IoEvent { Ping, Pong, Terminate, Menu, Play }
    ///
    ///     #[derive(Debug, Clone)]
    ///     struct AppData { event: Rc<RefCell<stream::FromIter<std::vec::IntoIter<IoEvent>>>> }
    ///
    ///     async fn ping<'s>(comp: &'s mut AppComposite, score: Score) -> Result<AppTransit<'s>, AppError> {
    ///         let mut score = score + 1;
    ///         let event = comp.data.event.clone();
    ///         while let Some(event) = (*event).borrow_mut().next().await {
    ///             match event {
    ///                 IoEvent::Pong => return Ok(Transit::To(Box::pin(pong(comp, score)))),
    ///                 _ => score += 1,
    ///             }
    ///         }
    ///         Ok(Transit::Lift(score))
    ///     }
    ///
    ///     async fn pong<'s>(comp: &'s mut AppComposite, score: Score) -> Result<AppTransit<'s>, AppError> {
    ///         let mut score = score + 1;
    ///         let event = comp.data.event.clone();
    ///         while let Some(event) = (*event).borrow_mut().next().await {
    ///             match event {
    ///                 IoEvent::Ping => return Ok(Transit::To(Box::pin(ping(comp, score)))),
    ///                 _ => score += 1,
    ///             }
    ///         }
    ///         Ok(Transit::Lift(score))
    ///     }
    ///
    ///     #[test]
    ///     fn test_game() {
    ///         let sequence = vec![ IoEvent::Ping, IoEvent::Pong, IoEvent::Ping, IoEvent::Pong];
    ///         let event = Rc::new(RefCell::new(stream::from_iter(sequence)));
    ///         let start_score = 0;
    ///         let mut app = AppComposite::new(AppData { event: event });
    ///         let result: Result<Score, AppError> = task::block_on(app.init(ping, start_score));
    ///         assert_eq!(Ok(5), result);
    ///     }
    /// ```
    pub async fn init<'s, Factory, FactoryArg, Out, Err,  Fut>(&'s mut self, f: Factory, arg: FactoryArg)
                                                              -> Result<Out, Err>

        where Factory: FnOnce(&'s mut Self, FactoryArg) -> Fut,
              Fut: Future<Output=Result<Transit<'s, Self, Out, Err>, Err>>,
            Out: Sized + Copy
    {
        let mut trans = f(self, arg).await?;

        loop {
            trans = match trans {
                Transit::To(h) => h.await?,
                Transit::Lift(lift) => return Ok(lift)
            }
        }
    }
}
