// §4.7 TypeView `Auto.deduced`: the specialization `g<int>` and a variable initialised from it: their
// placeholders are deduced as `int`, which is sugar.
template <class T> auto g(T t) { return t; }
auto x = g(1);
