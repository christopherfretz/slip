#include "dep.h"

template <class T> int make(T);

namespace legacy {
decltype(make(0)) v = 0;
}
