#include "side.h"
// digest-kinds §11.1 property 1: a destructor's specification differs; it
// is in the resolved unit, not in the destructor's own unit.
struct S {
#ifdef B
  ~S() noexcept(true);
#else
  ~S() noexcept(false);
#endif
};
