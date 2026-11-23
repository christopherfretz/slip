#include "side.h"
// digest-kinds §11.4: an inheriting constructor takes the base
// constructor's default arguments (Kiln class/special/define.rs), so a
// different default is a difference of the base constructor's own unit;
// the use through D changes nothing of D's.
struct Base {
#ifdef B
  Base(int = 2);
#else
  Base(int = 1);
#endif
};
struct D : Base { using Base::Base; };
D d;
void h() { D e(3); }
