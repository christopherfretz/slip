#include "side.h"
// WP-034 (§6): CoroutineBodyData `return_stmt`: the return statement converts the result variable, an xvalue, with another conversion function.
namespace std {
template <class R, class... A> struct coroutine_traits { using promise_type = typename R::promise_type; };
template <class P = void> struct coroutine_handle {
  void *p;
  static coroutine_handle from_address(void *a) noexcept { coroutine_handle h; h.p = a; return h; }
  void *address() const noexcept { return p; }
};
struct suspend_always {
  bool await_ready() const noexcept { return false; }
  template <class P> void await_suspend(coroutine_handle<P>) const noexcept {}
  void await_resume() const noexcept {}
};
struct suspend_never {
  bool await_ready() const noexcept { return true; }
  template <class P> void await_suspend(coroutine_handle<P>) const noexcept {}
  void await_resume() const noexcept {}
};
} // namespace std
#ifdef B
struct tk;
struct G { operator tk() const &; };
struct tk { struct promise_type {
  G get_return_object() { return {}; }
  std::suspend_never initial_suspend() noexcept { return {}; }
  std::suspend_never final_suspend() noexcept { return {}; }
  void return_void() {}
  void unhandled_exception() {}
}; };
tk f() { co_return; }
#else
struct tk;
struct G { operator tk() const &; operator tk() &&; };
struct tk { struct promise_type {
  G get_return_object() { return {}; }
  std::suspend_never initial_suspend() noexcept { return {}; }
  std::suspend_never final_suspend() noexcept { return {}; }
  void return_void() {}
  void unhandled_exception() {}
}; };
tk f() { co_return; }
#endif
