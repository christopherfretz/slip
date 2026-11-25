#include "side.h"
struct Base { static int s(); };
namespace n { struct S : Base {
#ifdef B
  static int h();
#else
  static int s();
#endif
}; }
// the same binding in a nested class template's member and in a member alias template
template <class T> struct i_N { template <class U> struct In { decltype(T::s(), U()) f; }; template <class U> using Al = decltype(T::s(), U()); };
i_N<n::S> i_n;
