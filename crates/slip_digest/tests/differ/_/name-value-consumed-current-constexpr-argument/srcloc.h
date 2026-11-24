namespace std { inline namespace __1 {
struct source_location {
  struct __impl { const char *_M_file_name; const char *_M_function_name; unsigned _M_line; unsigned _M_column; };
  static consteval source_location current(const __impl *p = __builtin_source_location()) noexcept {
    source_location s; s.impl = p; return s; }
  constexpr const char *function_name() const noexcept { return impl ? impl->_M_function_name : ""; }
  const __impl *impl = nullptr;
};
} }
