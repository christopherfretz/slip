#include "existing_h.h"
namespace acme { int g() { return 2; } }
int u() { return acme::helper() + acme::g(); }
