// WP-125 item 3 (digest §9, "Defaults at a use": a body written in place is its own evaluation context): a closure inside a constant evaluation (a constexpr variable's initializer), in a function and in a function template, consumes its __PRETTY_FUNCTION__'s length as an array bound while the constant's value does not depend on it; the namespace renamed (clang-19: 56 against 54 bytes in f, 59 against 57 in h<int>), so the array type of the closure's body differs in f and in h<int>'s definition
namespace legacy {
unsigned long f() { constexpr int n = [] { char a[sizeof(__PRETTY_FUNCTION__)]; return int(sizeof a > 1); }(); return n; }
template <class T> unsigned long h(T) { constexpr int n = [] { char a[sizeof(__PRETTY_FUNCTION__)]; return int(sizeof a > 1); }(); return n; }
unsigned long use() { return h(1); }
}
