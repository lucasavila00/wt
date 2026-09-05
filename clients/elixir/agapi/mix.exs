defmodule Agapi.MixProject do
  use Mix.Project

  def project do
    [
      app: :agapi,
      version: "0.1.0",
      elixir: "~> 1.20",
      deps: [{:jason, "~> 1.4"}, {:exile, "~> 0.14.0"}]
    ]
  end

  def application, do: [extra_applications: [:logger]]
end
