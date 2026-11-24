#include "new_dep.h"
namespace acme { template <class T> concept Small = sizeof(T) <= 4; }
template <class T> requires acme::Small<T> int f(T);
