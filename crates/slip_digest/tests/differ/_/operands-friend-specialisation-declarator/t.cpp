#include "side.h"
// WP-156 fix round 2 (X7), fix round 3 (moved from `indeterminate`: `kiln/friend-specialization-declarator` is deleted): a friend specialisation whose declarator holds an operand. `make(x)` binds to `lib::make` on side A and to `::make<lib::X>` on side B; its entry is a site-9 entry of the friend's operand unit (L3, L5, T5).
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
template <class T> struct Wr { T m; };
template <int N> struct W2 { int f() { return N; } };
template <class T> void tf(T);
struct F2 { friend void tf<>(Wr<decltype(make(x))>); };
