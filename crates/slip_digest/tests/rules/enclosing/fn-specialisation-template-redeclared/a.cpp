namespace __attribute__((visibility("default"))) lib {
  template <class T> struct enable { using type = void; };
  template <class T> typename enable<T>::type sw(T &a, T &b) noexcept;
}
namespace __attribute__((visibility("default"))) lib {
  template <class T> typename enable<T>::type sw(T &a, T &b) noexcept { T t = a; a = b; b = t; }
}
namespace __attribute__((visibility("default"))) my { struct T { int v; }; }
namespace __attribute__((visibility("hidden"))) lib {
  template <> void sw(my::T &a, my::T &b) noexcept { int v = a.v; a.v = b.v; b.v = v; }
}
void use(my::T &a, my::T &b) { lib::sw(a, b); }
