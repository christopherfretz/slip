#include "ro_inc.h"
#include "own_open.h"
namespace acme { int g(int); }
using namespace acme;
int u() { return g(1); }
