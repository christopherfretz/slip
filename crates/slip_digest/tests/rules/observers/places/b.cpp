int gv;
  namespace old { void f(); }
#include "d.h"
namespace fresh { struct C { void m(); }; namespace inner2 { struct Z {}; } }
namespace fresh { static_assert(sizeof(long) > 0); }
int u() { old::f(); return sizeof(fresh::C) + sizeof(fresh::inner2::Z) + old::sh(); }
