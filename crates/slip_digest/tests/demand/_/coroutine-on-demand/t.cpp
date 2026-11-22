// std: c++20
#include "side.h"
// §11.9 (WP-026: a row §11.9 must gain): `0-is-coroutine` is set when the body of an instantiation is analysed.
namespace std {
template <class R, class... A> struct coroutine_traits { using promise_type = typename R::promise_type; };
template <class P = void> struct coroutine_handle { static coroutine_handle from_address(void*); void* address(); };
}
struct S { bool await_ready() noexcept; template <class H> void await_suspend(H) noexcept; void await_resume() noexcept; };
struct R { struct promise_type { R get_return_object(); S initial_suspend(); S final_suspend() noexcept; void return_void(); void unhandled_exception(); }; };
template <class T> R c(T) { co_return; }
using Y = decltype(c(1));
#ifdef B
R r = c(1);
#else

#endif
