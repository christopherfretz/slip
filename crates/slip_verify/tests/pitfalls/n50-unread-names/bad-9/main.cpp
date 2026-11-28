#include "dep.h"

template <class T> int make(T);
template <class T> struct Wr { T m; };

namespace acme {
using v8 = int __attribute__((ext_vector_type(sizeof(make(0)) * 2)));
v8 ev;
}
