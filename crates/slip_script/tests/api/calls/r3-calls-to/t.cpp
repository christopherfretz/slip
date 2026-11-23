namespace n {
int f(int);
template <class T>
T tf(T x) {
  return x;
}
int v;
}
int a = n::f(2);
#include "h.h"
int b = n::f(n::f(3)) + n::tf(4) + n::tf(5.0);
