use crate::message::MetricPoints;
use crate::TEdgeNetDataCollectorBuilder;
use netdata_plugin::collector::Collector;
use netdata_plugin::Chart;
use netdata_plugin::Dimension;
use std::collections::HashMap;
use std::collections::HashSet;
use tedge_actors::Actor;
use tedge_actors::LoggingReceiver;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;

pub struct TEdgeNetDataCollector {
    pub(crate) input: LoggingReceiver<MetricPoints>,
}

impl TEdgeNetDataCollector {
    pub fn builder() -> TEdgeNetDataCollectorBuilder {
        TEdgeNetDataCollectorBuilder::default()
    }
}

#[async_trait::async_trait]
impl Actor for TEdgeNetDataCollector {
    fn name(&self) -> &str {
        "NetData"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        let mut writer = std::io::stdout();
        let mut c = Collector::new(&mut writer);
        let mut charts = HashMap::new();

        while let Some(points) = self.input.recv().await {
            // Declare any new chart
            let updated_charts: HashSet<String> =
                points.iter().map(|p| p.chart_id.clone()).collect();
            for chart_id in updated_charts.iter() {
                if !charts.contains_key(chart_id) {
                    let chart = new_chart(chart_id);
                    c.add_chart(&chart).unwrap();
                    charts.insert(chart_id.to_string(), HashSet::new());
                }
            }

            // Declare any new dimension
            for p in points.iter() {
                if let Some(dims) = charts.get_mut(&p.chart_id) {
                    let dim_id = p.dimension_id.clone();
                    if !dims.contains(&dim_id) {
                        let dim = new_dim(&dim_id);
                        c.add_dimension(&p.chart_id, &dim).unwrap();
                        dims.insert(dim_id);
                    }
                }
            }

            // Publish the metrics
            for p in points {
                c.prepare_value(&p.chart_id, &p.dimension_id, p.value)
                    .unwrap();
            }
            for chart_id in updated_charts {
                c.commit_chart(&chart_id).unwrap();
            }
        }

        Ok(())
    }
}

fn new_chart(chart_id: &str) -> Chart {
    Chart {
        type_id: chart_id,
        name: chart_id,
        title: chart_id,
        units: "units",
        ..Default::default()
    }
}

fn new_dim(dim_id: &str) -> Dimension {
    Dimension {
        id: dim_id,
        name: dim_id,
        ..Default::default()
    }
}
