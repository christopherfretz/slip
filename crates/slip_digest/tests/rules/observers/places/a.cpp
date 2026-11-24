int gv;
  namespace old { void f(); }
#include "d.h"
#include "e.h"
namespace old { struct C { void m(); }; namespace inner2 { struct Z {}; } }
namespace old { static_assert(sizeof(long) > 0); }
int u() { old::f(); return sizeof(old::C) + sizeof(old::inner2::Z) + old::sh(); }
