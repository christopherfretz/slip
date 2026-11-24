#include "new_dep.h"
namespace acme { template <class T> concept Small = sizeof(T) <= 4; }
template <acme::Small T> int f(T);
