namespace std {
struct source_location {
  struct __impl { const char *_M_file_name; const char *_M_function_name; unsigned _M_line; unsigned _M_column; };
  static consteval source_location current(const void *p = __builtin_source_location()) noexcept {
    source_location s; s.impl = static_cast<const __impl *>(p); return s; }
  constexpr const char *function_name() const noexcept { return impl ? impl->_M_function_name : ""; }
  constexpr unsigned line() const noexcept { return impl ? impl->_M_line : 0; }
  const __impl *impl = nullptr;
};
}
