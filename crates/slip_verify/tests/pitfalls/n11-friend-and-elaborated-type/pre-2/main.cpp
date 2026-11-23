#include "dep.h"

namespace legacy {
struct Mine {
  struct Tag *p;
};
}

legacy::Tag *user(legacy::Mine m) { return m.p; }
