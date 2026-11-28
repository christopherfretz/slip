#include "side.h"
// WP-136 item 5 (`spec/digest.md` §2.1, §8): relocating the failing
// candidate's template under a declared rule gives equal digests.
#ifdef B
namespace new_ns {
struct S {};
template <class T> auto g(T t) -> decltype(t.size());
int g(...);
}
int u() { return new_ns::g(new_ns::S{}); }
#else
namespace old_ns {
struct S {};
template <class T> auto g(T t) -> decltype(t.size());
int g(...);
}
int u() { return old_ns::g(old_ns::S{}); }
#endif
