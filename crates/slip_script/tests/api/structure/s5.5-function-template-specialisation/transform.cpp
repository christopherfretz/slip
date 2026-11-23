// spec: §5.5
// As written and as resolved (spec/api.md §5.5): `callee_of` and
// `referent_of` give the specialisation that was called, which is a
// function and not the template, and has no written declaration; its
// origin is the template, so `references_to` and `calls_to` of the
// template hold both uses. `callee_reference_of` is the reference as
// written, and its referent is the callee.
#include <slip/meta.h>

using namespace slip;

consteval std::string num(unsigned long long n) {
  return n < 10 ? std::string(1, static_cast<char>('0' + n))
                : num(n / 10) + static_cast<char>('0' + n % 10);
}

consteval void transform() {
  info tf = lookup("tf")[0];
  report(severity::note, "references_to(tf): " + num(references_to(tf).size()));
  std::vector<info> calls = calls_to(tf);
  report(severity::note, "calls_to(tf): " + num(calls.size()));
  for (info c : calls) {
    info callee = callee_of(c);
    info ref = callee_reference_of(c);
    std::string s = callee == tf ? "the template" : "not the template";
    s += is_function(callee) ? ", a function" : ", not a function";
    s += is_template(callee) ? ", a template" : ", not a template";
    s += ", " + num(declarations_of(callee).size()) + " declarations";
    s += referent_of(ref) == callee ? ", the referent of its reference" : ", not the referent";
    report(extent_of(c), severity::note, s);
  }
}
