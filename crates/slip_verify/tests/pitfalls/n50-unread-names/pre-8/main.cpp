#include "dep.h"

template <class T> int make(T);
template <class T> struct Wr { T m; };

namespace legacy {
typedef int v4 __attribute__((vector_size(sizeof(make(0)) * 4)));
v4 vv;
}
