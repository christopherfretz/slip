namespace acme {
#include "inner.h"
int g(int);
}
int u() { return acme::g(1) + sizeof(acme::Inner); }
