#include "new_dep.h"
namespace acme { template <class T> concept Small = sizeof(T) <= 4; }
int f(acme::Small auto t);
