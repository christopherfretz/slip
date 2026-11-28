#include "side.h"
// WP-156 fix round 3, KT1 2.9 fact (b): in `template <> int Q<4>::s =
// make(x);` Kiln holds the initializer's names by the instantiated class
// `Q<4>`, not by the declaration. `s`'s stream walks `make(x)` (its unit
// changes); `Q<4>`'s unit holds the entry and does not walk it, so it is
// also a site-9 entry of `Q<4>`'s operand unit: a duplicate, not a second
// meaning.
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
template <int N> struct Q { static int s; };
template <int N> int Q<N>::s = 0;
template <> int Q<4>::s = make(x);
