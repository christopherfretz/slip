#include "dep.h"

namespace legacy {
class Mine {
  int s = 1;
  friend struct Dep;
};
}

int user() { return legacy::Dep::get(legacy::Mine()); }
