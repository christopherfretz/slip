#include "side.h"
// §7 SuspendKind: `Handle`: the fact differs.
namespace std {
template <class R, class... A> struct coroutine_traits { using promise_type = typename R::promise_type; };
template <class P = void> struct coroutine_handle {
  static coroutine_handle from_address(void *) noexcept; void *address() const noexcept;
  template <class Q> coroutine_handle(coroutine_handle<Q>) noexcept; coroutine_handle() noexcept;
};
template <> struct coroutine_handle<void> {
  static coroutine_handle from_address(void *) noexcept; void *address() const noexcept;
  coroutine_handle() noexcept; template <class Q> coroutine_handle(coroutine_handle<Q>) noexcept;
};
}
struct Never { bool await_ready() noexcept; void await_suspend(std::coroutine_handle<>) noexcept; void await_resume() noexcept; };
struct Maybe { bool await_ready() noexcept; bool await_suspend(std::coroutine_handle<>) noexcept; int await_resume() noexcept; };
struct Hand { bool await_ready() noexcept; std::coroutine_handle<> await_suspend(std::coroutine_handle<>) noexcept; int await_resume() noexcept; };
struct Task { struct promise_type {
  Task get_return_object(); Never initial_suspend() noexcept; Never final_suspend() noexcept;
  void return_void(); void unhandled_exception(); Never yield_value(int);
}; };
Never never; Maybe maybe; Hand hand;
#ifdef B
Task f() { co_await never; }
#else
Task f() { co_await hand; }
#endif
