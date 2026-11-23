// spec: §6.1 rule 1
// The grammar of a qualified name (spec/api.md §6.1 rule 1): `::`, or an
// optional leading `::` and identifiers separated by `::`. Template
// arguments, operator names and whitespace are `invalid_argument`; so are
// an empty component, a leading digit, a keyword or an alternative token,
// an ASCII byte other than a letter, a digit, `_` and `$`, and text that
// is not UTF-8. A component with `$` or a code point outside ASCII is
// `unsupported`, unless another component is malformed. An identifier with
// special meaning (`final`) is an identifier. Each line: the name, then
// how many entities it gives, or the error.
#include <slip/meta.h>

using namespace slip;

consteval std::string num(unsigned long long n) {
  return n < 10 ? std::string(1, static_cast<char>('0' + n))
                : num(n / 10) + static_cast<char>('0' + n % 10);
}

consteval std::string errc_name(errc c) {
  switch (c) {
    case errc::invalid_handle: return "invalid_handle";
    case errc::wrong_kind: return "wrong_kind";
    case errc::invalid_argument: return "invalid_argument";
    case errc::not_editable: return "not_editable";
    case errc::unsupported: return "unsupported";
    case errc::limit_exceeded: return "limit_exceeded";
  }
  return "errc " + num(static_cast<unsigned>(c));
}

consteval void transform() {
  const std::string_view names[] = {
      "a::b",       "::a::b",  "::",        "a::final",       "a<int>",
      "a::operator+", "a :: b", "",          "a::",            "::::a",
      "a::::b",     "1a",      "class",     "a::and",         "a$",
      "\xc3\xa9",   "\xc3\xa9 b", "a$::1x", std::string_view("\xff", 1), "a::c",
      "a::b ",      "::a",
  };
  unsigned i = 0;
  for (std::string_view name : names) {
    std::string what;
    try {
      what = num(lookup(name).size()) + " entities";
    } catch (const error& e) {
      what = errc_name(e.code());
    }
    report(severity::note, (i < 10 ? "0" : "") + num(i) + " [" + std::string(name) + "]: " + what);
    ++i;
  }
}
