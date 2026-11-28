#include "dep.h"

struct X { int b; };

namespace acme {
template <class T, class U> concept C = sizeof(T) == sizeof(U);
C<legacy::X> auto v = 1;
}
