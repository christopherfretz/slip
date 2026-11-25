#include "side.h"
// WP-156 fix round 1 (W7; `spec/digest.md` §2.4 "What Kiln does not
// keep"): a declaration written outside its class, whose declarator's
// qualifier names the class by a template-id with an operand. Kiln keeps
// no qualifier as written for a declarator: a declaration outside a class
// that a template-id with an operand, linked by no declaration, names is
// indeterminate in its operand unit (`kiln/declarator-qualifier`); so is
// one outside a specialisation whose non-type argument may have been
// written with an expression before its name (`Q<4>`, `Q4<4>`). An alias
// template's template-id is kept as a type (`AL<sizeof(make(0))>` names
// `Pl`, `A2<decltype(make(0))>` names `Q2<int>`: fix round 2).
namespace lib { int make(int); }
using lib::make;
template <int N> struct Q { static int s; };
template <int N> int Q<N>::s = 1;
template <> int Q<sizeof(make(0))>::s = 5;
struct Pl { static int s; };
template <int> using AL = Pl;
int AL<sizeof(make(0))>::s = 2;
template <class T> struct Q2 { static int s; };
template <class T> int Q2<T>::s = 1;
template <class T> using A2 = Q2<T>;
template <> int A2<decltype(make(0))>::s = 5;
namespace ns {
template <int N> struct Q4 { static int s; };
template <> int Q4<sizeof(make(0))>::s = 5;
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
template <> int A3<sizeof(make(0))>::s = 5;
struct Pl2 { static int s; };
template <class> using AT = Pl2;
int AT<int>::s = 3;
template <int N> struct O { struct I { static int s; }; };
template <> int O<sizeof(make(0))>::I::s = 1;
template <int N> struct QE { enum E { e1 }; };
using enum QE<sizeof(make(0))>::E;
struct QD { enum F { f1 }; };
QD qd;
using enum decltype(qd)::F;
