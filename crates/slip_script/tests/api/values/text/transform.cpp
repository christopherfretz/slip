// spec: §5.3
// Text and sequences (spec/api.md §5.3): text comes out as std::string,
// the bytes of the file, not transcoded (rule 1: the parameter's name is
// UTF-8 in t.cpp and comes out as those bytes); a sequence is in unit
// order with no duplicate (rule 2): the declarations of `f`, written in
// four places, one of them in an included file, numbered as they come.
#include <slip/meta.h>

using namespace slip;

consteval void transform() {
  info f = lookup("f")[0];
  for (info p : parameters_of(f)) report(severity::note, "parameter: " + identifier_of(p));
  std::vector<info> ds = declarations_of(f);
  for (unsigned i = 0; i < ds.size(); ++i)
    report(severity::note, std::string(1, static_cast<char>('0' + i)) + ": " + display_string_of(ds[i]));
}
