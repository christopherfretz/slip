#include "side.h"
// digest §6 _ `expr-qualifier`: a qualifier that changes nothing Kiln
// reads it for (§7.3) changes no digest. Namespace members; members of a
// base (`member_extra-qualified` is set only for a virtual function);
// and a redundant `this->` outside a template (pitfalls E07,
// `CXXThisExpr.aux-implicit`).
namespace ns { int x; }
struct A { int g(); int m; };
struct Bu : A { using A::g; };
A a; Bu bu;
namespace ns {
#ifdef B
int v1 = ::ns::x, v2 = x, v3 = x; }
int c1 = a.A::g();
int c2 = bu.A::g();
struct Bd : A { int n = A::m; };
struct Th { int m; int n = this->m; };
#else
int v1 = x, v2 = ns::x, v3 = ::ns::x; }
int c1 = a.g();
int c2 = bu.g();
struct Bd : A { int n = m; };
struct Th { int m; int n = m; };
#endif
