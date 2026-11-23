// spec: §11
// A subject of three files (spec/api.md §11, "any other file"): t.cpp
// includes lib.h beside it and inc/deep.h below it. Each declaration is
// shown with its place, in unit order (§6.4).
#include <slip/meta.h>

using namespace slip;

consteval void transform() {
  for (std::string_view name : {"lib::f", "lib::g", "lib::h"})
    for (info e : lookup(name))
      for (info d : declarations_of(e)) report(severity::note, display_string_of(d));
}
