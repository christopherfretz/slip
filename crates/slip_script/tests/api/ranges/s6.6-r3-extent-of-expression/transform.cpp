// spec: §6.6 rule 3
// `extent_of(expression)` (spec/api.md §6.6 rule 3): from the first token
// of the expression to its last: a call, from its callee to its `)`; an
// argument that is a call, with the blanks inside it; a parenthesised
// argument, with its parentheses; a call over two lines is not extended by
// what follows it.
#include <slip/meta.h>

using namespace slip;

consteval std::string num(unsigned long long n) {
  return n < 10 ? std::string(1, static_cast<char>('0' + n))
                : num(n / 10) + static_cast<char>('0' + n % 10);
}

consteval void transform() {
  std::vector<info> calls = calls_to(lookup("f")[0]);
  for (unsigned i = 0; i < calls.size(); ++i) {
    report(extent_of(calls[i]), severity::note, "call #" + num(i));
    std::vector<info> args = arguments_as_written(calls[i]);
    for (unsigned j = 0; j < args.size(); ++j)
      report(extent_of(args[j]), severity::note, "call #" + num(i) + " argument #" + num(j));
  }
}
