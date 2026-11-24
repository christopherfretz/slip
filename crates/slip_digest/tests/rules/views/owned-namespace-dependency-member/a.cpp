namespace acme {
#include "inner.h"
}
namespace legacy { int g(int); }
int u() { return legacy::g(1) + sizeof(acme::Inner); }
