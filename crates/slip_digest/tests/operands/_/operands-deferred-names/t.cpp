#include "side.h"
// WP-156 fix round 3 (L3, mutation 5): a destructor's `noexcept` operand
// is written by its `exception_spec` part (site 6: the specification is
// of the deferrable class). Its names (`make`, `x`) are also written at
// site 9 by the class's unit, which holds the destructor (L2): L3's deferral, read for the class, does not reach the part, a
// duplicate on both sides (not a wrong answer). This pins the count, so
// that a fix of the deferral shows here (site 9 to 0).
namespace lib { struct X {}; int make(X); }
lib::X x;
struct D { ~D() noexcept(noexcept(lib::make(x))) {} };
void g() { D d; }
