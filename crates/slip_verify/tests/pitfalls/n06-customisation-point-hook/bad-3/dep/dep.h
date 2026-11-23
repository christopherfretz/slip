namespace legacy {
template <class T> int probe(T t) {
  if constexpr (requires { hook(t); }) return hook(t);
  else return 0;
}
}
