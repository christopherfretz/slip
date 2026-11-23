#include "side.h"
// digest-kinds §11.4: as `inheriting-constructor-default-argument`, with
// a base that is an implicit instantiation: the instantiated default is
// the base constructor's `default_argument` unit, and the pattern's unit
// differs.
template <class T> struct Base {
#ifdef B
  Base(int, int = 2);
#else
  Base(int, int = 1);
#endif
};
struct D : Base<long> { using Base<long>::Base; };
void h() { D e(3); }
