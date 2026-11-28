#include "side.h"
// WP-156 fix round 3 (L1): a namespace qualifier added on one side
// (`make(x)` to `lib::make(x)`) in a place no stream walks (an explicit
// instantiation's own `noexcept`, T1). A namespace name decides nothing
// by itself: its entry does not take part, and `make`'s names the same
// function on both sides: equal.
namespace lib { struct X {}; int make(X); }
lib::X x;
template <class T> void g(T) noexcept(true) {}
#ifdef B
template void g<int>(int) noexcept(sizeof(lib::make(x)) == 4);
#else
template void g<int>(int) noexcept(sizeof(make(x)) == 4);
#endif
