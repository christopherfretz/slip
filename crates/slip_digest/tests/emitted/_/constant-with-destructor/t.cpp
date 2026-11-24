#include "side.h"
// WP-155: a variable whose initialiser is a constant but whose type has a
// destructor that is not trivial is emitted: its destructor is registered
// to run at exit. Through an array too.
struct D { constexpr D() {} ~D() {} };
constinit D d;
D ds[2];
// A constexpr variable is evaluated (`Value`); its destructor, constexpr
// but not trivial, is constant destruction, which clang does not
// register. The walk does not read constant destruction: marked, a mark
// too many (spec/landed/verify/WP-155.md §1); it is the case that only
// the destructor test marks.
struct E { constexpr E() {} constexpr ~E() {} };
constexpr E e;
