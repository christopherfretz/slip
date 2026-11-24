// WP-112 (digest §9): __PRETTY_FUNCTION__ passed on through a pointer into
// an initializer_list's backing array
namespace std {
template <class E> class initializer_list {
  const E *b = nullptr; decltype(sizeof 0) n = 0;
public:
  constexpr initializer_list() = default;
  constexpr const E *begin() const { return b; }
};
}
void take(const char *const *);
namespace legacy { void f() {
  static constexpr std::initializer_list<const char *> l = {__PRETTY_FUNCTION__};
  constexpr const char *const *p = l.begin();
  take(p);
} }
