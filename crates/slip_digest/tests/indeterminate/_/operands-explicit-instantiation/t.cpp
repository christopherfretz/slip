#include "side.h"
// WP-156 fix round 1 (W8; `spec/digest.md` §2.4 "What Kiln does not
// keep"): an explicit instantiation of a function, a variable or a member
// of a class template specialisation. Kiln keeps neither its template
// arguments nor its declarator as written: its operand unit is
// indeterminate (`kiln/explicit-instantiation-written`), with or without
// an operand. A class's explicit instantiation keeps its arguments (no
// line); a member its class's explicit instantiation covers has none.
namespace lib { int make(int); }
using lib::make;
template <class T> struct Wr { T m; };
template <int N> int ft() { return N; }
template int ft<sizeof(make(0))>();
template <class T> void fx(T) {}
template void fx(Wr<decltype(make(0))>);
template <class T> T vt = T();
template int vt<decltype(make(0))>;
template <int N> struct Q { int f() { return N; } };
template int Q<sizeof(make(0))>::f();
template <int N> struct R { int f() { return N; } };
template struct R<sizeof(make(0))>;
// Fix round 2, S-2: a member class's explicit instantiation (M11); a
// function's whose parameter type's qualifier holds the operand (M23).
template <int N> struct QI { struct In { int f() { return N; } }; };
template struct QI<sizeof(make(0))>::In;
template <int N> struct QN { struct In {}; };
template <class T> void fn(T) {}
template void fn(QN<sizeof(make(0))>::In);
