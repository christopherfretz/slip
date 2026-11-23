#include "dep.h"
namespace acme { struct Mine {}; }
namespace legacy { void touch(Dep &d) { d.v = 1; } }
