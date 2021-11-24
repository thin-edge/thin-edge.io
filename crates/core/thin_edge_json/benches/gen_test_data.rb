# Ruby script to generate large or deeply nested test data.

require 'json'

def float_or_num
  rand > 0.5 ? rand : (rand(100_000) - 50_000)
end

def gen_recursive(n)
  if n > 0
    {"i" => gen_recursive(n-1)}
  else
    0
  end
end

def gen(h, key_prefix, n)
  for i in 1...n do
    h["#{key_prefix}#{i}"] = yield
  end
  h
end

case ARGV.shift
when "large"
  puts JSON.pretty_generate(gen({}, "key_", 1000) { gen({}, "", 100) { float_or_num } })
when "huge"
  puts JSON.pretty_generate(gen({}, "key_", 1000) { gen({}, "", 1000) { float_or_num } })
when "nested"
  puts JSON.dump(gen_recursive(10000))
else
  raise "Usage: #{$0} [large | huge | nested]"
end
