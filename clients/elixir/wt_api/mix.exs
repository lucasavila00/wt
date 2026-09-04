defmodule WtApi.MixProject do
  use Mix.Project

  def project do
    [
      app: :wt_api,
      version: "0.1.0",
      elixir: "~> 1.20",
      start_permanent: Mix.env() == :prod,
      deps: deps()
    ]
  end

  def application do
    [extra_applications: [:logger]]
  end

  defp deps do
    [
      {:exile, "~> 0.14.0"},
      {:jason, "~> 1.4"}
    ]
  end
end
