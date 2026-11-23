// WP-112 R1, R2 (b) (digest §9): std::source_location::current() in a default member initializer (used by a constructor, a class template's constructor, an aggregate initialization; a default argument inside the initializer) is passed on at the use: the __impl object's function name is the user's, fed to the names digest
#include "srcloc.h"
void take(const void *);
struct Agg { int x; std::source_location l = std::source_location::current(); };
struct Rec { std::source_location l; };
constexpr Rec inner(std::source_location l = std::source_location::current()) { return {l}; }
struct Nest { Rec r = inner(); };
namespace legacy {
struct S { std::source_location l = std::source_location::current(); S() { take(l.function_name()); } };
template <class T> struct TS { std::source_location l = std::source_location::current(); TS() { take(l.function_name()); } };
void f() { S s; }
void g() { TS<int> t; }
void h() { Agg a{1}; take(a.l.function_name()); }
void k() { Nest n{}; take(n.r.l.function_name()); }
}
