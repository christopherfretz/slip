#include "side.h"
// WP-156 fix round 1 (W7), fix round 2 (S-2), fix round 3 (moved from `indeterminate`: `kiln/declarator-qualifier` is deleted): a declarator's qualifier with an operand, through a template-id, an alias template-id, an outer specialisation, a `using enum`. `make(x)` binds to `lib::make` on side A and to `::make<lib::X>` on side B; its entry is a site-9 entry of the declaration's operand unit (L3, L5).
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
template <int N> struct Q { static int s; };
template <int N> int Q<N>::s = 1;
template <> int Q<sizeof(make(x))>::s = 5;
struct Pl { static int s; };
template <int> using AL = Pl;
int AL<sizeof(make(x))>::s = 2;
template <class T> struct Q2 { static int s; };
template <class T> int Q2<T>::s = 1;
template <class T> using A2 = Q2<T>;
template <> int A2<decltype(make(x))>::s = 5;
namespace ns {
template <int N> struct Q4 { static int s; };
template <> int Q4<sizeof(make(x))>::s = 5;
}
// Fix round 2, S-2 (the review's surviving mutants): an alias template-id
// whose arguments hold an operand, naming a specialisation through a type
// argument (M03); an alias template-id with a type argument alone, whose
// alias is the operand (M09); a qualifier whose outer class is the
// specialisation (M15); a `using enum` through a template-id or a
// `decltype` with an operand (M10).
template <class T> struct Wr { T m; };
template <class T> struct Q3 { static int s; };
template <class T> int Q3<T>::s = 1;
template <int N> using A3 = Q3<Wr<char[N]>>;
template <> int A3<sizeof(make(x))>::s = 5;
struct Pl2 { static int s; };
template <class> using AT = Pl2;
int AT<int>::s = 3;
template <int N> struct O { struct I { static int s; }; };
template <> int O<sizeof(make(x))>::I::s = 1;
template <int N> struct QE { enum E { e1 }; };
using enum QE<sizeof(make(x))>::E;
struct QD { enum F { f1 }; };
QD qd;
using enum decltype(qd)::F;
