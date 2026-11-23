#include "dep.h"

namespace acme {
struct Mine {
  struct Tag *p;
};
}

acme::Tag *user(acme::Mine m) { return m.p; }
