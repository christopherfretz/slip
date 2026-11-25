#include "side.h"
// WP-156 (WP-153 §13.2 B7 item 2, found closed at the pin): an adjusted
// parameter of an instantiated function keeps its type before adjustment
// (`ParamData::original_ty`), so `decltype(T::s()) q[2]` is read and the
// name in it is seen (`n::S::s` on side A, `Base::s` on side B under the
// rename): the operand unit of the specialisation differs.
struct Base { static int s(); };
namespace n { struct S : Base {
#ifdef B
  static int h();
#else
  static int s();
#endif
}; }
template <class T> void p(T, decltype(T::s()) q[2]);
void u() { p(n::S(), nullptr); }
