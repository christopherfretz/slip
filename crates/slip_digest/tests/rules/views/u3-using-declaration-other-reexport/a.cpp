#include "new_dep.h"
namespace legacy { int g(int); }
namespace other { using legacy::g; }
using legacy::g;
