#include "side.h"
struct Base { static int s(); };
namespace n { struct S : Base {
#ifdef B
  static int h();
#else
  static int s();
#endif
}; }
template <class T> struct i_M { template <class U> auto g(U u) -> decltype(T::s(), u); int g(int); };
i_M<n::S> i_m;
int i_u = i_m.g(1);
