#include "new_dep.h"
namespace acme { int g(int); }
namespace other { using acme::g; }
using other::g;
