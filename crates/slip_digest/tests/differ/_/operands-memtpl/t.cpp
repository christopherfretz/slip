#include "side.h"
struct Base { static int s(); };
namespace n { struct S : Base {
#ifdef B
  static int h();
#else
  static int s();
#endif
}; }
template <class T> struct i_M { template <class U> auto g(U u) -> decltype(T::s(), u); };
i_M<n::S> i_m;
