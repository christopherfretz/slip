#include "side.h"
// WP-163 fix round 2 (B2): a nested class of the partial specialisation P<T, 0> defined out of line, read as `converted-arg-out-of-line-function`.
namespace n { constexpr int k = 0; constexpr int j = 0; }
#ifdef B
template <class T, int I> struct P { struct In; };
template <class T> struct P<T, 0> { struct In; };
template <class T> struct P<T, n::j>::In { int m; };
#else
template <class T, int I> struct P { struct In; };
template <class T> struct P<T, 0> { struct In; };
template <class T> struct P<T, n::k>::In { int m; };
#endif
