#include "side.h"
// WP-156 fix round 1 (W8), fix round 2 (S-2), fix round 3 (moved from `indeterminate`: `kiln/explicit-instantiation-written` is deleted): explicit instantiations of a function, a variable, a member and a member class with an operand. `make(x)` binds to `lib::make` on side A and to `::make<lib::X>` on side B; an entry the translation unit holds is a site-9 entry of `<order>`'s operand unit (L3, L5, T1).
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
template <class T> struct Wr { T m; };
template <int N> int ft() { return N; }
template int ft<sizeof(make(x))>();
template <class T> void fx(T) {}
template void fx(Wr<decltype(make(x))>);
template <class T> T vt = T();
template int vt<decltype(make(x))>;
template <int N> struct Q { int f() { return N; } };
template int Q<sizeof(make(x))>::f();
template <int N> struct R { int f() { return N; } };
template struct R<sizeof(make(x))>;
// Fix round 2, S-2: a member class's explicit instantiation (M11); a
// function's whose parameter type's qualifier holds the operand (M23).
template <int N> struct QI { struct In { int f() { return N; } }; };
template struct QI<sizeof(make(x))>::In;
template <int N> struct QN { struct In {}; };
template <class T> void fn(T) {}
template void fn(QN<sizeof(make(x))>::In);
