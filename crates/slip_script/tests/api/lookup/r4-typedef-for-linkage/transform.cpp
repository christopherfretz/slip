// spec: §6.1 rule 4
// A typedef name for linkage purposes ([dcl.typedef]/9) names its unnamed
// class as a scope (spec/api.md §6.1 rule 4): `T::m` is found, and `T`
// itself is the typedef, a type. A typedef of a named class is not a scope
// (rule 5: type aliases are not followed): `V::k` is not found, `U::k` is.
#include <slip/meta.h>

using namespace slip;

consteval std::string num(unsigned long long n) {
  return n < 10 ? std::string(1, static_cast<char>('0' + n))
                : num(n / 10) + static_cast<char>('0' + n % 10);
}

consteval void transform() {
  for (std::string_view name : {"T", "T::m", "U::k", "V::k"})
    report(severity::note, std::string(name) + ": " + num(lookup(name).size()));
  report(severity::note, is_type(lookup("T")[0]) ? "T is a type" : "T is not a type");
}
