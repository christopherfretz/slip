#include "side.h"
// §6 StmtNode `CoreturnStmt.b`: the promise call selects another conversion; the operand is the same.
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
struct vtask { struct promise_type {
  vtask get_return_object() { return {}; }
  std::suspend_never initial_suspend() noexcept { return {}; }
  std::suspend_never final_suspend() noexcept { return {}; }
  void return_value(int) {}
  void return_value(long) {}
  void unhandled_exception() {}
}; };
#ifdef B
struct wt { operator long(); };
vtask f() { co_return wt{}; }
#else
struct wt { operator int(); };
vtask f() { co_return wt{}; }
#endif
