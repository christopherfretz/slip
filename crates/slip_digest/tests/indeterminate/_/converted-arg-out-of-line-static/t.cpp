#include "side.h"
// WP-163 fix round 2 (B2): a static data member of the partial specialisation P<T, 0> defined out of line, read as `converted-arg-out-of-line-function`.
namespace n { constexpr int k = 0; constexpr int j = 0; }
#ifdef B
template <class T, int I> struct P { static int s; };
template <class T> struct P<T, 0> { static int s; };
template <class T> int P<T, n::j>::s = 0;
#else
template <class T, int I> struct P { static int s; };
template <class T> struct P<T, 0> { static int s; };
template <class T> int P<T, n::k>::s = 0;
#endif
