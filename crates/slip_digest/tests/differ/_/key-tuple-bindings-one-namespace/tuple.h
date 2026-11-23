namespace std {
template <class T> struct tuple_size;
template <unsigned long I, class T> struct tuple_element;
}
struct T { template <unsigned long I> int get() const { return 0; } };
template <> struct std::tuple_size<T> { static constexpr unsigned long value = 2; };
template <unsigned long I> struct std::tuple_element<I, T> { using type = int; };
