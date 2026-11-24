// WP-112 R1, R2 (a) (digest §9): __builtin_source_location() in a default member initializer (used by a constructor, a class template's aggregate initialization (WP-124 makes a class template's constructor body indeterminate), an aggregate initialization; a default argument inside the initializer) is the operand of no immediate invocation, passed on at the use: the __impl object's function name is the user's, fed to the names digest
#include "srcloc.h"
void take(const void *);
struct Agg { int x; const void *p = __builtin_source_location(); };
constexpr const void *inner(const void *p = __builtin_source_location()) { return p; }
struct Nest { const void *p = inner(); };
namespace acme {
struct S { const void *p = __builtin_source_location(); S() { take(p); } };
template <class T> struct TS { const void *p = __builtin_source_location(); };
void f() { S s; }
void g() { TS<int> t{}; take(t.p); }
void h() { Agg a{1}; take(a.p); }
void k() { Nest n{}; take(n.p); }
}
