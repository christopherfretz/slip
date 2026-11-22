#include "side.h"
// §6 CoroutineBodyData `return_on_alloc_failure`: the promise has `get_return_object_on_allocation_failure` (the promise type is the same class on both sides).
namespace std { struct nothrow_t { explicit nothrow_t() = default; }; extern const nothrow_t nothrow; }
void *operator new(decltype(sizeof 0), const std::nothrow_t &) noexcept;
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
struct ta { struct promise_type {
  ta get_return_object() { return {}; }
  std::suspend_never initial_suspend() noexcept { return {}; }
  std::suspend_never final_suspend() noexcept { return {}; }
  void return_void() {}
  void unhandled_exception() {}
  static ta get_return_object_on_allocation_failure() { return {}; }
}; };
ta f() { co_return; }
#else
struct ta { struct promise_type {
  ta get_return_object() { return {}; }
  std::suspend_never initial_suspend() noexcept { return {}; }
  std::suspend_never final_suspend() noexcept { return {}; }
  void return_void() {}
  void unhandled_exception() {}
}; };
ta f() { co_return; }
#endif
