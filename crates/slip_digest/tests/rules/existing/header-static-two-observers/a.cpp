#include "existing_h.h"
namespace legacy { int g() { return 2; } }
int u() { return acme::helper() + legacy::g(); }
