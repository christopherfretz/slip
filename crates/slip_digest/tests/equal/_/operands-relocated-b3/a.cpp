// WP-156 item 5 (B3): the definition of a class made by instantiation
// lists its member templates' keys; a member template's partly
// substituted signature names the relocated namespace, so its key holds
// it. A correct relocation maps the key: every unit is equal.
namespace legacy { struct S { static int s(); }; }
template <class T> struct i_M {
  template <class U> auto g(U u) -> decltype(T::s(), u);
  template <class U> struct In { decltype(T::s(), U()) f; };
  template <class U> using Al = decltype(T::s(), U());
};
i_M<legacy::S> i_m;
int i_u = sizeof(i_m.g(1));
