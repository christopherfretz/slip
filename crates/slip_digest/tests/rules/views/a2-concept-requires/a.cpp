#include "new_dep.h"
namespace legacy { template <class T> concept Small = sizeof(T) <= 4; }
template <class T> requires legacy::Small<T> int f(T);
