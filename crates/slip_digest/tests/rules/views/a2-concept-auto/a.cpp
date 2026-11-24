#include "new_dep.h"
namespace legacy { template <class T> concept Small = sizeof(T) <= 4; }
int f(legacy::Small auto t);
