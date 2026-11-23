#include "h.h"
void f();
namespace n {
int a;
}
namespace n {
int b;
}
void f() {}
template <class T>
T tf(T x) {
  return x;
}
int y = tf(1);
