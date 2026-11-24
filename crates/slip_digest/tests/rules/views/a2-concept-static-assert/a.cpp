#include "new_dep.h"
namespace legacy { template <class T> concept Small = sizeof(T) <= 4; }
template <class T> struct S { static_assert(legacy::Small<T>); };
