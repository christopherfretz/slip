#include "side.h"
// digest-kinds §11.9: `definition` writes the parameters before the body,
// so a reference to the second parameter is told from one to the first.
template <class T> struct Q { int g(int a, int b) {
#ifdef B
  return a;
#else
  return b;
#endif
} };
Q<int> q;
int h() { return q.g(1, 2); }
